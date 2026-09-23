use std::cell::Cell;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use tempfile::TempDir;

use super::{run_watch_plan_with_prompts, watch_execution_cwd};
use crate::planning::{CwdState, PlanningOutcome, PlanningRequest, PreparedPlanner};
use crate::runtime::RuntimeContext;
use crate::watch::protocol::InputFrame;
use aegis_config::AegisConfig;
use aegis_policy::ExecutionTransport;
use aegis_snapshot::{GitPlugin, SnapshotRegistry};
use aegis_tui::{PromptDecision, RecoveryPromptDecision};

#[test]
fn watch_execution_cwd_returns_resolved_path() {
    let path = PathBuf::from("/srv/project");
    let cwd_state = CwdState::Resolved(path.clone());

    assert_eq!(watch_execution_cwd(&cwd_state), path);
}

#[test]
fn watch_execution_cwd_returns_dot_when_unavailable() {
    let cwd_state = CwdState::Unavailable;

    assert_eq!(watch_execution_cwd(&cwd_state), PathBuf::from("."));
}

/// Build a [`PreparedPlanner`] whose snapshot registry is pinned to `GitPlugin`
/// only, so it cannot pick up a `docker`-applicable state from whatever
/// containers happen to be running on the host (see
/// `set_snapshot_registry_for_tests`).
fn prepared_with_audit_path(audit_path: PathBuf) -> PreparedPlanner {
    let context = RuntimeContext::new_with_audit_path(
        AegisConfig::default(),
        tokio::runtime::Handle::current(),
        audit_path,
    )
    .unwrap();
    context.set_snapshot_registry_for_tests(SnapshotRegistry::new_with_plugins(vec![Box::new(
        GitPlugin,
    )]));
    PreparedPlanner::Ready(Box::new(context))
}

/// Pinned to an empty registry, not `GitPlugin`-only: this test asserts what
/// `sandbox_status` gets recorded on a Recovery-Deny path, not anything about
/// `GitPlugin` itself, so it has no reason to spawn `git` at all. See the
/// ADR-039 addendum for why a `GitPlugin`-only registry was suspected (but not
/// confirmed) to still race under process/FD pressure via
/// `GitPlugin::is_applicable`'s fail-open branch.
fn prepared_with_optional_sandbox(audit_path: PathBuf) -> PreparedPlanner {
    let mut config = AegisConfig::default();
    config.sandbox.enabled = true;
    let context =
        RuntimeContext::new_with_audit_path(config, tokio::runtime::Handle::current(), audit_path)
            .unwrap();
    context.set_snapshot_registry_for_tests(SnapshotRegistry::new_with_plugins(vec![]));
    PreparedPlanner::Ready(Box::new(context))
}

/// A snapshot plugin that always applies and always succeeds.
struct SucceedingPlugin;

#[async_trait::async_trait]
impl aegis_snapshot::SnapshotPlugin for SucceedingPlugin {
    fn name(&self) -> &'static str {
        "mock-succeeding"
    }

    async fn is_applicable(&self, _cwd: &std::path::Path) -> bool {
        true
    }

    async fn snapshot(
        &self,
        _cwd: &std::path::Path,
        _cmd: &str,
    ) -> Result<String, aegis_snapshot::SnapshotError> {
        Ok("mock-snapshot-id".to_string())
    }

    async fn rollback(&self, _snapshot_id: &str) -> Result<(), aegis_snapshot::SnapshotError> {
        Ok(())
    }

    async fn delete(&self, _snapshot_id: &str) -> Result<(), aegis_snapshot::SnapshotError> {
        Ok(())
    }
}

/// A snapshot plugin that always applies and always fails.
struct FailingPlugin;

#[async_trait::async_trait]
impl aegis_snapshot::SnapshotPlugin for FailingPlugin {
    fn name(&self) -> &'static str {
        "mock-failing"
    }

    async fn is_applicable(&self, _cwd: &std::path::Path) -> bool {
        true
    }

    async fn snapshot(
        &self,
        _cwd: &std::path::Path,
        _cmd: &str,
    ) -> Result<String, aegis_snapshot::SnapshotError> {
        Err(aegis_snapshot::SnapshotError::Snapshot(
            "mock plugin failure".to_string(),
        ))
    }

    async fn rollback(&self, _snapshot_id: &str) -> Result<(), aegis_snapshot::SnapshotError> {
        Ok(())
    }

    async fn delete(&self, _snapshot_id: &str) -> Result<(), aegis_snapshot::SnapshotError> {
        Ok(())
    }
}

/// Pin the registry to one plugin that succeeds and one that fails, so the
/// snapshot pass produces exactly the partial coverage of issue #312: a usable
/// snapshot record next to an applicable plugin that never produced one.
fn prepared_with_partial_coverage(audit_path: PathBuf) -> PreparedPlanner {
    let context = RuntimeContext::new_with_audit_path(
        AegisConfig::default(),
        tokio::runtime::Handle::current(),
        audit_path,
    )
    .unwrap();
    context.set_snapshot_registry_for_tests(SnapshotRegistry::new_with_plugins(vec![
        Box::new(SucceedingPlugin),
        Box::new(FailingPlugin),
    ]));
    PreparedPlanner::Ready(Box::new(context))
}

async fn effect_opaque_plan(
    prepared: &PreparedPlanner,
    workspace: &TempDir,
) -> (InputFrame, crate::planning::InterceptionPlan) {
    let command = "sh ./run.sh";
    let cwd = workspace.path().to_string_lossy().into_owned();
    let outcome = prepared
        .plan_async(PlanningRequest {
            command,
            cwd_state: CwdState::Resolved(workspace.path().to_path_buf()),
            transport: ExecutionTransport::Watch,
            ci_detected: false,
        })
        .await;
    let PlanningOutcome::Planned(plan) = outcome else {
        panic!("expected an interception plan");
    };
    (
        InputFrame {
            cmd: command.to_string(),
            cwd: Some(cwd),
            interactive: None,
            source: Some("test".to_string()),
            id: Some("recovery-test".to_string()),
        },
        plan,
    )
}

fn read_audit_entry(path: &std::path::Path) -> Value {
    let contents = fs::read_to_string(path).unwrap();
    serde_json::from_str(contents.trim()).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_recovery_prompt_deny_prevents_execution_and_audits_degradation() {
    let workspace = TempDir::new().unwrap();
    let audit_dir = TempDir::new().unwrap();
    let audit_path = audit_dir.path().join("audit.jsonl");
    fs::write(workspace.path().join("run.sh"), "printf ran > executed\n").unwrap();
    let prepared = prepared_with_audit_path(audit_path.clone());
    let (frame, plan) = effect_opaque_plan(&prepared, &workspace).await;
    let prompted: Cell<Option<aegis_types::RecoveryDegradation>> = Cell::new(None);

    run_watch_plan_with_prompts(
        frame,
        &prepared,
        plan,
        false,
        |_, _| PromptDecision::Approve,
        |degradation| {
            prompted.set(Some(degradation));
            RecoveryPromptDecision::Deny
        },
    )
    .await;

    assert_eq!(
        prompted.get(),
        Some(aegis_types::RecoveryDegradation::NoSnapshotAvailable)
    );
    assert!(!workspace.path().join("executed").exists());
    let entry = read_audit_entry(&audit_path);
    assert_eq!(entry["decision"], "Denied");
    assert_eq!(entry["recovery_degradation"], "no_snapshot_available");
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_recovery_prompt_run_once_executes_and_audits_degradation() {
    let workspace = TempDir::new().unwrap();
    let audit_dir = TempDir::new().unwrap();
    let audit_path = audit_dir.path().join("audit.jsonl");
    fs::write(workspace.path().join("run.sh"), "printf ran > executed\n").unwrap();
    let prepared = prepared_with_audit_path(audit_path.clone());
    let (frame, plan) = effect_opaque_plan(&prepared, &workspace).await;
    let prompted: Cell<Option<aegis_types::RecoveryDegradation>> = Cell::new(None);

    run_watch_plan_with_prompts(
        frame,
        &prepared,
        plan,
        false,
        |_, _| PromptDecision::Approve,
        |degradation| {
            prompted.set(Some(degradation));
            RecoveryPromptDecision::RunOnceWithoutRecovery
        },
    )
    .await;

    assert_eq!(
        prompted.get(),
        Some(aegis_types::RecoveryDegradation::NoSnapshotAvailable)
    );
    assert!(workspace.path().join("executed").exists());
    let entry = read_audit_entry(&audit_path);
    assert_eq!(entry["decision"], "Approved");
    assert_eq!(entry["recovery_degradation"], "no_snapshot_available");
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_partial_snapshot_coverage_prompts_and_audits_its_own_reason() {
    let workspace = TempDir::new().unwrap();
    let audit_dir = TempDir::new().unwrap();
    let audit_path = audit_dir.path().join("audit.jsonl");
    fs::write(workspace.path().join("run.sh"), "printf ran > executed\n").unwrap();
    let prepared = prepared_with_partial_coverage(audit_path.clone());
    let (frame, plan) = effect_opaque_plan(&prepared, &workspace).await;
    let prompted: Cell<Option<aegis_types::RecoveryDegradation>> = Cell::new(None);

    run_watch_plan_with_prompts(
        frame,
        &prepared,
        plan,
        false,
        |_, _| PromptDecision::Approve,
        |degradation| {
            prompted.set(Some(degradation));
            RecoveryPromptDecision::Deny
        },
    )
    .await;

    assert_eq!(
        prompted.get(),
        Some(aegis_types::RecoveryDegradation::PartialSnapshotCoverage)
    );
    assert!(!workspace.path().join("executed").exists());
    let entry = read_audit_entry(&audit_path);
    assert_eq!(entry["decision"], "Denied");
    assert_eq!(entry["recovery_degradation"], "partial_snapshot_coverage");
    assert_eq!(entry["snapshots"][0]["plugin"], "mock-succeeding");
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_recovery_deny_records_enabled_sandbox_as_not_attempted() {
    let workspace = TempDir::new().unwrap();
    let audit_dir = TempDir::new().unwrap();
    let audit_path = audit_dir.path().join("audit.jsonl");
    fs::write(workspace.path().join("run.sh"), "printf ran > executed\n").unwrap();
    let prepared = prepared_with_optional_sandbox(audit_path.clone());
    let (frame, plan) = effect_opaque_plan(&prepared, &workspace).await;

    run_watch_plan_with_prompts(
        frame,
        &prepared,
        plan,
        false,
        |_, _| PromptDecision::Approve,
        |_| RecoveryPromptDecision::Deny,
    )
    .await;

    let entry = read_audit_entry(&audit_path);
    assert_eq!(entry["sandbox_status"], "not_attempted");
}
