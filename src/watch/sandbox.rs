//! Sandbox lifecycle for Watch commands.

use std::future::Future;

use aegis_types::{RecoveryDegradation, SandboxStatus};

use crate::planning::InterceptionPlan;
use crate::runtime::{RuntimeContext, WatchAuditContext};
use aegis_types::Decision;
use aegis_types::{SnapshotAttempt, SnapshotRecord};

use super::protocol::{InputFrame, OutputDecision, OutputFrame, emit_frame};

pub(super) struct WatchExecution<'a> {
    pub(super) frame: &'a InputFrame,
    pub(super) context: &'a RuntimeContext,
    pub(super) plan: &'a InterceptionPlan,
    pub(super) ci_detected: bool,
    pub(super) cwd: Option<&'a std::path::Path>,
    pub(super) snapshots: &'a [SnapshotRecord],
    pub(super) attempts: &'a [SnapshotAttempt],
    pub(super) recovery_degradation: Option<RecoveryDegradation>,
}

#[derive(Debug, PartialEq, Eq)]
enum WatchSandboxEvent {
    RequiredBlocked,
    RequiredBlockedNested,
    SetupFailed(String),
}

async fn complete_watch_sandbox_lifecycle<C, AuditError, SpawnFuture>(
    decision: Decision,
    preparation: Result<(C, SandboxStatus), aegis_sandbox::SandboxError>,
    mut append_audit: impl FnMut(Decision, SandboxStatus) -> Result<(), AuditError>,
    mut emit_event: impl FnMut(WatchSandboxEvent),
    spawn: impl FnOnce(C) -> SpawnFuture,
    report_audit_error: impl FnOnce(&AuditError),
) where
    SpawnFuture: Future<Output = ()>,
{
    match preparation {
        Ok((command, status)) => {
            if let Err(err) = append_audit(decision, status) {
                report_audit_error(&err);
                return;
            }
            spawn(command).await;
        }
        Err(aegis_sandbox::SandboxError::Required) => {
            if let Err(err) = append_audit(Decision::Blocked, SandboxStatus::Unavailable) {
                report_audit_error(&err);
                return;
            }
            emit_event(WatchSandboxEvent::RequiredBlocked);
        }
        Err(aegis_sandbox::SandboxError::RequiredNestedUnderOuterSandbox) => {
            if let Err(err) = append_audit(Decision::Blocked, SandboxStatus::Unavailable) {
                report_audit_error(&err);
                return;
            }
            emit_event(WatchSandboxEvent::RequiredBlockedNested);
        }
        Err(err) => {
            if let Err(audit_err) = append_audit(Decision::Blocked, SandboxStatus::NotAttempted) {
                report_audit_error(&audit_err);
                return;
            }
            emit_event(WatchSandboxEvent::SetupFailed(err.to_string()));
        }
    }
}

pub(super) async fn complete_watch_approved_execution(
    execution: WatchExecution<'_>,
    decision: Decision,
) {
    let cmd = execution.frame.cmd.clone();
    let config = execution.context.effective_config().clone();
    let cwd = execution.cwd.map(std::path::Path::to_path_buf);
    let preparation = tokio::task::spawn_blocking(move || {
        let sandbox = frame_sandbox(&config, cwd.as_deref());
        prepare_watch_command_blocking(&cmd, sandbox.as_ref())
    })
    .await
    .map_err(|err| {
        aegis_sandbox::SandboxError::Execution(format!(
            "Watch Sandbox preparation task failed: {err}"
        ))
    })
    .and_then(|result| result);
    let event_id = execution.frame.id.clone();
    let spawn_id = execution.frame.id.clone();
    let audit_id = execution.frame.id.clone();
    let cwd = execution.cwd;

    complete_watch_sandbox_lifecycle(
        decision,
        preparation,
        |final_decision, sandbox_status| {
            append_watch_execution_audit(&execution, final_decision, sandbox_status)
        },
        |event| emit_watch_sandbox_event(event, &event_id),
        |command| super::runner::execute_prepared_and_emit(command, cwd, spawn_id),
        |err| emit_watch_audit_error(&audit_id, err),
    )
    .await;
}

/// The session-startup unavailability verdict (ADR-029 §3): `Some` warning
/// fields when a configured Sandbox cannot confine on this host, `None` when
/// confinement is not configured (nothing is expected) or the probe succeeds.
/// Split from the emitter so the selection is testable on any host.
pub(super) fn startup_unavailability(
    sandbox: Option<&aegis_sandbox::SandboxConfig>,
    is_wsl1: bool,
    apparmor_restricted: bool,
    available: impl FnOnce(&aegis_sandbox::SandboxConfig) -> bool,
) -> Option<(&'static str, &'static str)> {
    let config = sandbox?;
    if available(config) {
        return None;
    }
    Some(crate::runtime::sandbox_unavailable_code_and_message(
        is_wsl1,
        apparmor_restricted,
    ))
}

#[cfg(test)]
static TEST_STARTUP_UNAVAILABLE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Warn once at watch-session startup when a configured Sandbox has no usable
/// bwrap path (ADR-029 §3: warn at startup, refuse per command). Emitted
/// before the first input frame so the unavailability is visible even when
/// the session's first commands never reach execution.
pub(super) async fn warn_if_sandbox_unavailable_at_startup(context: &RuntimeContext) {
    let sandbox = context.config().sandbox.clone();
    // The probe spawns and waits on a real bwrap child, which must not block
    // the async runtime thread (CONVENTION.md §3); this module's prepare path
    // already routes blocking sandbox work through spawn_blocking.
    let unavailability = tokio::task::spawn_blocking(move || {
        let is_wsl1 = aegis_sandbox::wsl1_unavailable();
        let apparmor_restricted = aegis_sandbox::apparmor_userns_restricted();
        let available: fn(&aegis_sandbox::SandboxConfig) -> bool =
            aegis_sandbox::sandbox_available_for;
        #[cfg(test)]
        let (is_wsl1, apparmor_restricted, available) =
            if TEST_STARTUP_UNAVAILABLE.load(std::sync::atomic::Ordering::SeqCst) {
                (
                    false,
                    true,
                    (|_| false) as fn(&aegis_sandbox::SandboxConfig) -> bool,
                )
            } else {
                (is_wsl1, apparmor_restricted, available)
            };
        startup_unavailability(sandbox.as_ref(), is_wsl1, apparmor_restricted, available)
    })
    .await
    // A join failure means the probe never answered; the startup warning is
    // advisory, so it fails open here — per-command refusal still gates
    // execution.
    .ok()
    .flatten();
    let Some((code, message)) = unavailability else {
        return;
    };
    if emit_frame(&OutputFrame::Warning {
        id: None,
        code,
        sandbox_status: SandboxStatus::Unavailable,
        message,
    })
    .is_err()
    {
        std::process::exit(4);
    }
}

fn emit_watch_sandbox_event(event: WatchSandboxEvent, id: &Option<String>) {
    let result = match event {
        WatchSandboxEvent::RequiredBlocked => emit_frame(&OutputFrame::SandboxResult {
            id: id.clone(),
            decision: OutputDecision::Blocked,
            exit_code: 3,
            code: crate::runtime::SANDBOX_REQUIRED_UNAVAILABLE_CODE,
            sandbox_status: SandboxStatus::Unavailable,
            message: crate::runtime::SANDBOX_REQUIRED_UNAVAILABLE_MESSAGE,
        }),
        WatchSandboxEvent::RequiredBlockedNested => emit_frame(&OutputFrame::SandboxResult {
            id: id.clone(),
            decision: OutputDecision::Blocked,
            exit_code: 3,
            code: crate::runtime::SANDBOX_REQUIRED_NESTED_UNAVAILABLE_CODE,
            sandbox_status: SandboxStatus::Unavailable,
            message: crate::runtime::SANDBOX_REQUIRED_NESTED_UNAVAILABLE_MESSAGE,
        }),
        WatchSandboxEvent::SetupFailed(err) => emit_frame(&OutputFrame::Error {
            id: id.clone(),
            exit_code: 4,
            message: format!("Sandbox setup failed; command not executed: {err}"),
        }),
    };
    if result.is_err() {
        std::process::exit(4);
    }
}

pub(super) fn append_watch_execution_audit(
    execution: &WatchExecution<'_>,
    decision: Decision,
    sandbox_status: SandboxStatus,
) -> Result<(), crate::error::AegisError> {
    let watch = WatchAuditContext {
        allowlist_match: execution.plan.decision_context().allowlist_match(),
        allowlist_effective: execution.plan.policy_decision().allowlist_effective,
        ci_detected: execution.ci_detected,
        sandbox_status,
        snapshot_attempts: execution.attempts,
        source: execution.frame.source.clone(),
        cwd: execution.cwd.and_then(super::runner::audit_cwd),
        id: execution.frame.id.clone(),
    };
    match execution.recovery_degradation {
        Some(degradation) => execution
            .context
            .append_watch_audit_entry_with_recovery_degradation(
                execution.plan.assessment(),
                decision,
                execution.snapshots,
                execution.plan.explanation(),
                watch,
                degradation,
            ),
        None => execution.context.append_watch_audit_entry(
            execution.plan.assessment(),
            decision,
            execution.snapshots,
            execution.plan.explanation(),
            watch,
        ),
    }
}

pub(super) fn emit_watch_audit_error(id: &Option<String>, err: &crate::error::AegisError) {
    if emit_frame(&OutputFrame::Error {
        id: id.clone(),
        exit_code: 4,
        message: format!("audit log write failed: {err}"),
    })
    .is_err()
    {
        std::process::exit(4);
    }
}

pub(super) fn sandbox_status_before_preparation(context: &RuntimeContext) -> SandboxStatus {
    if context.config().sandbox.is_some() {
        SandboxStatus::NotAttempted
    } else {
        SandboxStatus::NotConfigured
    }
}

fn frame_sandbox(
    config: &aegis_config::AegisConfig,
    cwd: Option<&std::path::Path>,
) -> Option<aegis_sandbox::SandboxConfig> {
    if config.mode == aegis_types::Mode::Audit {
        return None;
    }
    let canonical = cwd.and_then(|path| std::fs::canonicalize(path).ok());
    let mut sandbox = crate::runtime::sandbox_ceiling(
        config,
        canonical.as_deref().unwrap_or(std::path::Path::new("")),
    )?
    .config;
    if canonical.is_none() {
        let mut default = config.clone();
        default.sandbox.allow_write = None;
        let temporary = crate::runtime::sandbox_ceiling(&default, std::path::Path::new(""))?.config;
        sandbox.allow_write = sandbox
            .allow_write
            .into_iter()
            .flat_map(|root| {
                let canonical_root = std::fs::canonicalize(&root.path).ok();
                temporary.allow_write.iter().filter_map(move |temp| {
                    let path = canonical_root.as_ref()?;
                    let temp = std::fs::canonicalize(&temp.path).ok()?;
                    let intersection = if path.starts_with(&temp) {
                        path
                    } else if temp.starts_with(path) {
                        &temp
                    } else {
                        return None;
                    };
                    let mut root = root.clone();
                    root.path = intersection.to_path_buf();
                    Some(root)
                })
            })
            .collect();
    }
    Some(sandbox)
}

pub(super) async fn prepare_watch_command(
    cmd: &str,
    sandbox: Option<&aegis_sandbox::SandboxConfig>,
) -> Result<(std::process::Command, SandboxStatus), aegis_sandbox::SandboxError> {
    let cmd = cmd.to_owned();
    let sandbox = sandbox.cloned();
    tokio::task::spawn_blocking(move || prepare_watch_command_blocking(&cmd, sandbox.as_ref()))
        .await
        .map_err(|err| {
            aegis_sandbox::SandboxError::Execution(format!(
                "Watch Sandbox preparation task failed: {err}"
            ))
        })?
}

fn prepare_watch_command_blocking(
    cmd: &str,
    sandbox: Option<&aegis_sandbox::SandboxConfig>,
) -> Result<(std::process::Command, SandboxStatus), aegis_sandbox::SandboxError> {
    let shell = std::env::var_os("AEGIS_REAL_SHELL")
        .or_else(|| std::env::var_os("SHELL"))
        .unwrap_or_else(|| "/bin/sh".into());
    let args = [
        std::ffi::OsString::from("-c"),
        std::ffi::OsString::from(cmd),
    ];

    if let Some(config) = sandbox {
        let prepared = aegis_sandbox::prepare_for_spawn(config, shell.as_os_str(), &args)?;
        return Ok((prepared.command, prepared.status));
    }

    let mut command = std::process::Command::new(shell);
    command.args(args);
    Ok((command, SandboxStatus::NotConfigured))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[tokio::test]
    async fn frame_ceiling_uses_frame_worktree() {
        let worktree = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(worktree.path().join(".git")).unwrap();
        let context = RuntimeContext::new_with_audit_path(
            aegis_config::AegisConfig::default(),
            tokio::runtime::Handle::current(),
            worktree.path().join("audit.jsonl"),
        )
        .unwrap();
        let startup = context.config().sandbox.as_ref().unwrap();
        let process_cwd = std::env::current_dir().unwrap();
        let same = frame_sandbox(context.effective_config(), Some(&process_cwd)).unwrap();
        assert_eq!(same.allow_write, startup.allow_write);
        assert_eq!(same.trusted_roots, startup.trusted_roots);
        assert_eq!(same.allow_network, startup.allow_network);
        let sandbox = frame_sandbox(context.effective_config(), Some(worktree.path())).unwrap();
        assert!(
            !sandbox
                .allow_write
                .iter()
                .any(|root| root.path == process_cwd)
        );
        assert!(
            sandbox
                .allow_write
                .iter()
                .any(|root| root.path == std::fs::canonicalize(worktree.path()).unwrap())
        );
    }

    #[tokio::test]
    async fn frame_ceiling_uses_effective_config() {
        let worktree = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(worktree.path().join(".git")).unwrap();
        let mut config = aegis_config::AegisConfig::default();
        config.sandbox.allow_write = Some(vec![]);
        let context = RuntimeContext::new_with_audit_path(
            config,
            tokio::runtime::Handle::current(),
            worktree.path().join("audit.jsonl"),
        )
        .unwrap();
        let sandbox = frame_sandbox(context.effective_config(), Some(worktree.path())).unwrap();
        assert!(sandbox.allow_write.is_empty());
    }

    #[test]
    fn frame_ceiling_canonicalizes_relative_and_symlink_cwd() {
        use std::os::unix::fs::symlink;
        let base = tempfile::tempdir_in(".").unwrap();
        let repo = base.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let link = base.path().join("link");
        symlink("repo", &link).unwrap();
        let expected = std::fs::canonicalize(&repo).unwrap();
        for cwd in [&repo, &link] {
            let sandbox = frame_sandbox(&aegis_config::AegisConfig::default(), Some(cwd)).unwrap();
            assert!(sandbox.allow_write.iter().any(|root| root.path == expected));
        }
    }

    #[test]
    fn missing_frame_cwd_keeps_only_temporary_roots() {
        let missing = std::path::Path::new("/aegis-nonexistent-frame-cwd");
        let sandbox = frame_sandbox(&aegis_config::AegisConfig::default(), Some(missing)).unwrap();
        assert!(
            sandbox
                .allow_write
                .iter()
                .all(|root| root.path.starts_with("/tmp")
                    || root.path.starts_with("/private/tmp")
                    || root.path.starts_with("/private/var/folders")
                    || root.path.starts_with("/var/folders"))
        );
        assert!(!sandbox.allow_write.is_empty());
    }

    #[test]
    fn missing_frame_cwd_never_grants_configured_non_temp_roots() {
        use aegis_config::ceiling::WriteRoot;
        use aegis_types::CeilingLayer;
        let mut config = aegis_config::AegisConfig::default();
        config.sandbox.allow_write = Some(vec![WriteRoot::new(
            "/var/tmp".into(),
            CeilingLayer::Global,
            "global.toml",
        )]);
        let missing = std::path::Path::new("/aegis-nonexistent-frame-cwd");
        let sandbox = frame_sandbox(&config, Some(missing)).unwrap();
        assert!(sandbox.allow_write.is_empty());
        config.sandbox.allow_write = Some(vec![WriteRoot::new(
            "/".into(),
            CeilingLayer::Global,
            "global.toml",
        )]);
        let sandbox = frame_sandbox(&config, Some(missing)).unwrap();
        assert!(
            sandbox
                .allow_write
                .iter()
                .any(|root| root.path == std::fs::canonicalize("/tmp").unwrap())
        );
        config.sandbox.allow_write = Some(vec![]);
        assert!(
            frame_sandbox(&config, Some(missing))
                .unwrap()
                .allow_write
                .is_empty()
        );
    }

    #[test]
    fn audit_frame_has_no_sandbox() {
        let mut config = aegis_config::AegisConfig::default();
        config.mode = aegis_types::Mode::Audit;
        assert!(frame_sandbox(&config, Some(std::path::Path::new("/nonexistent"))).is_none());
    }

    #[test]
    fn unavailable_warning_selects_the_wsl1_code_and_message_on_wsl1() {
        let (code, message) = crate::runtime::sandbox_unavailable_code_and_message(true, false);
        assert_eq!(code, crate::runtime::SANDBOX_WSL1_UNAVAILABLE_CODE);
        assert_eq!(message, crate::runtime::SANDBOX_WSL1_UNAVAILABLE_MESSAGE);

        let (code, message) = crate::runtime::sandbox_unavailable_code_and_message(false, false);
        assert_eq!(code, crate::runtime::SANDBOX_REQUIRED_UNAVAILABLE_CODE);
        assert_eq!(
            message,
            crate::runtime::SANDBOX_REQUIRED_UNAVAILABLE_MESSAGE
        );
    }

    #[test]
    fn startup_warning_is_emitted() {
        const CHILD: &str = "AEGIS_TEST_STARTUP_WARNING_CHILD";
        if std::env::var_os(CHILD).is_some() {
            super::TEST_STARTUP_UNAVAILABLE.store(true, std::sync::atomic::Ordering::SeqCst);
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let dir = tempfile::TempDir::new().unwrap();
                let context = crate::runtime::RuntimeContext::new_with_audit_path(
                    aegis_config::AegisConfig::default(),
                    tokio::runtime::Handle::current(),
                    dir.path().join("audit.jsonl"),
                )
                .unwrap();
                warn_if_sandbox_unavailable_at_startup(&context).await;
            });
        } else {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "watch::sandbox::tests::startup_warning_is_emitted",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let stdout = String::from_utf8(output.stdout).unwrap();
            let frame = stdout
                .lines()
                .find_map(|line| {
                    line.find('{')
                        .and_then(|i| serde_json::from_str::<serde_json::Value>(&line[i..]).ok())
                })
                .expect("startup warning frame");
            assert_eq!(frame["type"], "warning");
            assert_eq!(frame["code"], "sandbox_apparmor_unavailable");
            assert_eq!(frame["sandbox_status"], "unavailable");
            assert_eq!(
                frame["message"],
                crate::runtime::SANDBOX_APPARMOR_UNAVAILABLE_MESSAGE
            );
        }
    }

    #[test]
    fn apparmor_unavailability_selector_and_watch_warning() {
        let config = aegis_sandbox::SandboxConfig::default();
        let expected = (
            "sandbox_apparmor_unavailable",
            crate::runtime::SANDBOX_APPARMOR_UNAVAILABLE_MESSAGE,
        );
        assert_eq!(
            crate::runtime::sandbox_unavailable_code_and_message(false, true),
            expected
        );
        assert_eq!(
            startup_unavailability(Some(&config), false, true, |_| false),
            Some(expected)
        );
        assert_eq!(
            crate::runtime::sandbox_unavailable_code_and_message(true, true).0,
            "sandbox_wsl1_unavailable"
        );
        assert_eq!(
            crate::runtime::sandbox_unavailable_code_and_message(false, false).0,
            "sandbox_required_unavailable"
        );
    }

    #[test]
    fn startup_unavailability_is_none_when_sandbox_is_not_configured() {
        assert!(startup_unavailability(None, false, false, |_| false).is_none());
    }

    #[test]
    fn startup_unavailability_is_none_when_the_probe_succeeds() {
        let config = aegis_sandbox::SandboxConfig::default();
        assert!(startup_unavailability(Some(&config), false, false, |_| true).is_none());
    }

    #[test]
    fn startup_unavailability_warns_generically_when_not_wsl1() {
        let config = aegis_sandbox::SandboxConfig::default();
        let (code, message) =
            startup_unavailability(Some(&config), false, false, |_| false).expect("must warn");
        assert_eq!(code, crate::runtime::SANDBOX_REQUIRED_UNAVAILABLE_CODE);
        assert_eq!(
            message,
            crate::runtime::SANDBOX_REQUIRED_UNAVAILABLE_MESSAGE
        );
    }

    #[test]
    fn startup_unavailability_names_wsl1_when_the_host_is_wsl1() {
        let config = aegis_sandbox::SandboxConfig::default();
        let (code, message) =
            startup_unavailability(Some(&config), true, false, |_| false).expect("must warn");
        assert_eq!(code, crate::runtime::SANDBOX_WSL1_UNAVAILABLE_CODE);
        assert_eq!(message, crate::runtime::SANDBOX_WSL1_UNAVAILABLE_MESSAGE);
    }

    #[tokio::test]
    async fn required_unavailability_audits_block_and_never_spawns() {
        let events = RefCell::new(Vec::new());

        complete_watch_sandbox_lifecycle(
            Decision::Approved,
            Err::<((), SandboxStatus), _>(aegis_sandbox::SandboxError::Required),
            |decision, status| {
                events
                    .borrow_mut()
                    .push(format!("audit:{decision:?}:{status:?}"));
                Ok::<_, String>(())
            },
            |event| events.borrow_mut().push(format!("event:{event:?}")),
            |()| async {
                events.borrow_mut().push("spawn".to_string());
            },
            |err| events.borrow_mut().push(format!("audit-error:{err}")),
        )
        .await;

        assert_eq!(
            events.into_inner(),
            ["audit:Blocked:Unavailable", "event:RequiredBlocked"]
        );
    }

    #[tokio::test]
    async fn required_nested_unavailability_audits_block_and_never_spawns() {
        let events = RefCell::new(Vec::new());

        complete_watch_sandbox_lifecycle(
            Decision::Approved,
            Err::<((), SandboxStatus), _>(
                aegis_sandbox::SandboxError::RequiredNestedUnderOuterSandbox,
            ),
            |decision, status| {
                events
                    .borrow_mut()
                    .push(format!("audit:{decision:?}:{status:?}"));
                Ok::<_, String>(())
            },
            |event| events.borrow_mut().push(format!("event:{event:?}")),
            |()| async {
                events.borrow_mut().push("spawn".to_string());
            },
            |err| events.borrow_mut().push(format!("audit-error:{err}")),
        )
        .await;

        assert_eq!(
            events.into_inner(),
            ["audit:Blocked:Unavailable", "event:RequiredBlockedNested"]
        );
    }

    #[tokio::test]
    async fn setup_failure_audits_not_attempted_and_never_spawns() {
        let events = RefCell::new(Vec::new());

        complete_watch_sandbox_lifecycle(
            Decision::Approved,
            Err::<((), SandboxStatus), _>(aegis_sandbox::SandboxError::SetupFailed(
                "invalid profile".to_string(),
            )),
            |decision, status| {
                events
                    .borrow_mut()
                    .push(format!("audit:{decision:?}:{status:?}"));
                Ok::<_, String>(())
            },
            |event| events.borrow_mut().push(format!("event:{event:?}")),
            |()| async {
                events.borrow_mut().push("spawn".to_string());
            },
            |err| events.borrow_mut().push(format!("audit-error:{err}")),
        )
        .await;

        assert_eq!(
            events.into_inner(),
            [
                "audit:Blocked:NotAttempted",
                "event:SetupFailed(\"sandbox setup failed: invalid profile\")",
            ]
        );
    }

    #[tokio::test]
    async fn audit_failure_prevents_spawn() {
        let events = RefCell::new(Vec::new());

        complete_watch_sandbox_lifecycle(
            Decision::AutoApproved,
            Ok(((), SandboxStatus::NotConfigured)),
            |_decision, _status| Err::<(), _>("permission denied".to_string()),
            |event| events.borrow_mut().push(format!("event:{event:?}")),
            |()| async {
                events.borrow_mut().push("spawn".to_string());
            },
            |err| events.borrow_mut().push(format!("audit-error:{err}")),
        )
        .await;

        assert_eq!(events.into_inner(), ["audit-error:permission denied"]);
    }

    #[tokio::test]
    async fn unconfigured_sandbox_audits_and_spawns_without_warning() {
        let events = RefCell::new(Vec::new());

        complete_watch_sandbox_lifecycle(
            Decision::AutoApproved,
            Ok(((), SandboxStatus::NotConfigured)),
            |decision, status| {
                events
                    .borrow_mut()
                    .push(format!("audit:{decision:?}:{status:?}"));
                Ok::<_, String>(())
            },
            |event| events.borrow_mut().push(format!("event:{event:?}")),
            |()| async {
                events.borrow_mut().push("spawn".to_string());
            },
            |err| events.borrow_mut().push(format!("audit-error:{err}")),
        )
        .await;

        assert_eq!(
            events.into_inner(),
            ["audit:AutoApproved:NotConfigured", "spawn"]
        );
    }
}
