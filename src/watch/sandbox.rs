//! Sandbox lifecycle for Watch commands.

use std::future::Future;

use aegis_types::{RecoveryDegradation, SandboxStatus};

use crate::audit::Decision;
use crate::planning::InterceptionPlan;
use crate::runtime::{RuntimeContext, WatchAuditContext};
use crate::snapshot::SnapshotRecord;

use super::protocol::{InputFrame, OutputDecision, OutputFrame, emit_frame};

pub(super) struct WatchExecution<'a> {
    pub(super) frame: &'a InputFrame,
    pub(super) context: &'a RuntimeContext,
    pub(super) plan: &'a InterceptionPlan,
    pub(super) ci_detected: bool,
    pub(super) cwd: &'a std::path::Path,
    pub(super) snapshots: &'a [SnapshotRecord],
    pub(super) recovery_degradation: Option<RecoveryDegradation>,
}

#[derive(Debug, PartialEq, Eq)]
enum WatchSandboxEvent {
    Warning,
    RequiredBlocked,
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
            if status == SandboxStatus::Unavailable {
                emit_event(WatchSandboxEvent::Warning);
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
    let preparation = prepare_watch_command(
        &execution.frame.cmd,
        execution.context.config().sandbox.as_ref(),
    )
    .await;
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

/// The unavailable-warning frame fields, naming the WSL1 cause when it applies
/// so the operator gets a practical remedy (use WSL2) rather than the generic
/// message (ADR-029 §3). Split out so both branches are testable on any host.
fn unavailable_warning_code_and_message(is_wsl1: bool) -> (&'static str, &'static str) {
    if is_wsl1 {
        (
            crate::runtime::SANDBOX_WSL1_UNAVAILABLE_CODE,
            crate::runtime::SANDBOX_WSL1_UNAVAILABLE_MESSAGE,
        )
    } else {
        (
            crate::runtime::SANDBOX_UNAVAILABLE_CODE,
            crate::runtime::SANDBOX_UNAVAILABLE_MESSAGE,
        )
    }
}

/// The session-startup unavailability verdict (ADR-029 §3): `Some` warning
/// fields when a configured Sandbox cannot confine on this host, `None` when
/// confinement is not configured (nothing is expected) or the probe succeeds.
/// Split from the emitter so the selection is testable on any host.
pub(super) fn startup_unavailability(
    sandbox: Option<&aegis_sandbox::SandboxConfig>,
    is_wsl1: bool,
    available: impl FnOnce(&aegis_sandbox::SandboxConfig) -> bool,
) -> Option<(&'static str, &'static str)> {
    let config = sandbox?;
    if available(config) {
        return None;
    }
    Some(unavailable_warning_code_and_message(is_wsl1))
}

/// Warn once at watch-session startup when a configured Sandbox has no usable
/// bwrap path (ADR-029 §3: warn at startup, refuse per command). Emitted
/// before the first input frame so the unavailability is visible even when
/// the session's first commands never reach execution.
pub(super) async fn warn_if_sandbox_unavailable_at_startup(context: &RuntimeContext) {
    let sandbox = context.config().sandbox.clone();
    // The probe spawns and waits on a real bwrap child, which must not block
    // the async runtime thread (CLAUDE.md); this module's prepare path
    // already routes blocking sandbox work through spawn_blocking.
    let unavailability = tokio::task::spawn_blocking(move || {
        startup_unavailability(
            sandbox.as_ref(),
            aegis_sandbox::wsl1_unavailable(),
            aegis_sandbox::sandbox_available_for,
        )
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
        WatchSandboxEvent::Warning => {
            let (code, message) =
                unavailable_warning_code_and_message(aegis_sandbox::wsl1_unavailable());
            emit_frame(&OutputFrame::Warning {
                id: id.clone(),
                code,
                sandbox_status: SandboxStatus::Unavailable,
                message,
            })
        }
        WatchSandboxEvent::RequiredBlocked => emit_frame(&OutputFrame::SandboxResult {
            id: id.clone(),
            decision: OutputDecision::Blocked,
            exit_code: 3,
            code: crate::runtime::SANDBOX_REQUIRED_UNAVAILABLE_CODE,
            sandbox_status: SandboxStatus::Unavailable,
            message: crate::runtime::SANDBOX_REQUIRED_UNAVAILABLE_MESSAGE,
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
        source: execution.frame.source.clone(),
        cwd: execution.frame.cwd.clone(),
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

    #[test]
    fn unavailable_warning_selects_the_wsl1_code_and_message_on_wsl1() {
        let (code, message) = unavailable_warning_code_and_message(true);
        assert_eq!(code, crate::runtime::SANDBOX_WSL1_UNAVAILABLE_CODE);
        assert_eq!(message, crate::runtime::SANDBOX_WSL1_UNAVAILABLE_MESSAGE);

        let (code, message) = unavailable_warning_code_and_message(false);
        assert_eq!(code, crate::runtime::SANDBOX_UNAVAILABLE_CODE);
        assert_eq!(message, crate::runtime::SANDBOX_UNAVAILABLE_MESSAGE);
    }

    #[test]
    fn startup_unavailability_is_none_when_sandbox_is_not_configured() {
        assert!(startup_unavailability(None, false, |_| false).is_none());
    }

    #[test]
    fn startup_unavailability_is_none_when_the_probe_succeeds() {
        let config = aegis_sandbox::SandboxConfig::default();
        assert!(startup_unavailability(Some(&config), false, |_| true).is_none());
    }

    #[test]
    fn startup_unavailability_warns_generically_when_not_wsl1() {
        let config = aegis_sandbox::SandboxConfig::default();
        let (code, message) =
            startup_unavailability(Some(&config), false, |_| false).expect("must warn");
        assert_eq!(code, crate::runtime::SANDBOX_UNAVAILABLE_CODE);
        assert_eq!(message, crate::runtime::SANDBOX_UNAVAILABLE_MESSAGE);
    }

    #[test]
    fn startup_unavailability_names_wsl1_when_the_host_is_wsl1() {
        let config = aegis_sandbox::SandboxConfig::default();
        let (code, message) =
            startup_unavailability(Some(&config), true, |_| false).expect("must warn");
        assert_eq!(code, crate::runtime::SANDBOX_WSL1_UNAVAILABLE_CODE);
        assert_eq!(message, crate::runtime::SANDBOX_WSL1_UNAVAILABLE_MESSAGE);
    }

    #[tokio::test]
    async fn optional_unavailability_audits_then_warns_then_spawns() {
        let events = RefCell::new(Vec::new());

        complete_watch_sandbox_lifecycle(
            Decision::AutoApproved,
            Ok(((), SandboxStatus::Unavailable)),
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
            ["audit:AutoApproved:Unavailable", "event:Warning", "spawn",]
        );
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
    async fn audit_failure_prevents_warning_and_spawn() {
        let events = RefCell::new(Vec::new());

        complete_watch_sandbox_lifecycle(
            Decision::AutoApproved,
            Ok(((), SandboxStatus::Unavailable)),
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
