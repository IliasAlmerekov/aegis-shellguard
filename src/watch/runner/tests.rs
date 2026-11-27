use std::cell::Cell;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use tempfile::TempDir;

use super::run_watch_plan_with_prompts;
use crate::planning::{CwdState, PlanningOutcome, PlanningRequest, plan_with_context_async};
use crate::runtime::RuntimeContext;
use crate::watch::protocol::InputFrame;
use crate::watch::sandbox::audit_cwd;
use aegis_config::AegisConfig;
use aegis_policy::ExecutionTransport;
use aegis_snapshot::{GitPlugin, SnapshotRegistry};
use aegis_tui::{PromptDecision, RecoveryPromptDecision};

fn context_with_audit_path(audit_path: PathBuf) -> RuntimeContext {
    let context = RuntimeContext::new_with_audit_path(
        AegisConfig::default(),
        tokio::runtime::Handle::current(),
        audit_path,
    )
    .unwrap();
    context.set_snapshot_registry_for_tests(SnapshotRegistry::new_with_plugins(vec![Box::new(
        GitPlugin,
    )]));
    context
}

fn context_with_optional_sandbox(audit_path: PathBuf) -> RuntimeContext {
    let config = AegisConfig::default();
    let context =
        RuntimeContext::new_with_audit_path(config, tokio::runtime::Handle::current(), audit_path)
            .unwrap();
    context.set_snapshot_registry_for_tests(SnapshotRegistry::new_with_plugins(vec![]));
    context
}

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

    async fn readiness(&self, _snapshot_id: &str) -> aegis_snapshot::ArtifactReadiness {
        aegis_snapshot::ArtifactReadiness::PRESENT_UNCHECKED
    }

    async fn liveness(
        &self,
        _snapshot_id: &str,
    ) -> Result<aegis_snapshot::Liveness, aegis_snapshot::SnapshotError> {
        Ok(aegis_snapshot::Liveness::Present)
    }
}

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

    async fn readiness(&self, _snapshot_id: &str) -> aegis_snapshot::ArtifactReadiness {
        aegis_snapshot::ArtifactReadiness::PRESENT_UNCHECKED
    }

    async fn liveness(
        &self,
        _snapshot_id: &str,
    ) -> Result<aegis_snapshot::Liveness, aegis_snapshot::SnapshotError> {
        Ok(aegis_snapshot::Liveness::Present)
    }
}

fn context_with_partial_coverage(audit_path: PathBuf) -> RuntimeContext {
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
    context
}

async fn effect_opaque_plan(
    context: &RuntimeContext,
    workspace: &TempDir,
) -> (InputFrame, crate::planning::InterceptionPlan) {
    plan_for_command(context, workspace, "sh ./run.sh").await
}

async fn plan_for_command(
    context: &RuntimeContext,
    workspace: &TempDir,
    command: &str,
) -> (InputFrame, crate::planning::InterceptionPlan) {
    let cwd = workspace.path().to_string_lossy().into_owned();
    let outcome = plan_with_context_async(
        context,
        PlanningRequest {
            command,
            cwd_state: CwdState::Resolved(workspace.path().to_path_buf()),
            transport: ExecutionTransport::Watch,
            ci_detected: false,
        },
    )
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
    let context = context_with_audit_path(audit_path.clone());
    let (frame, plan) = effect_opaque_plan(&context, &workspace).await;
    let prompted: Cell<Option<aegis_types::RecoveryDegradation>> = Cell::new(None);

    run_watch_plan_with_prompts(
        frame,
        &context,
        plan,
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
    let context = context_with_audit_path(audit_path.clone());
    let (frame, plan) = effect_opaque_plan(&context, &workspace).await;
    let prompted: Cell<Option<aegis_types::RecoveryDegradation>> = Cell::new(None);

    run_watch_plan_with_prompts(
        frame,
        &context,
        plan,
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
    let context = context_with_partial_coverage(audit_path.clone());
    let (frame, plan) = effect_opaque_plan(&context, &workspace).await;
    let prompted: Cell<Option<aegis_types::RecoveryDegradation>> = Cell::new(None);

    run_watch_plan_with_prompts(
        frame,
        &context,
        plan,
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
    let context = context_with_optional_sandbox(audit_path.clone());
    let (frame, plan) = effect_opaque_plan(&context, &workspace).await;

    run_watch_plan_with_prompts(
        frame,
        &context,
        plan,
        |_, _| PromptDecision::Approve,
        |_| RecoveryPromptDecision::Deny,
    )
    .await;

    let entry = read_audit_entry(&audit_path);
    assert_eq!(entry["sandbox_status"], "not_attempted");
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_confirmed_run_with_an_unresolved_git_target_skips_the_recovery_prompt() {
    let workspace = TempDir::new().unwrap();
    let audit_dir = TempDir::new().unwrap();
    let audit_path = audit_dir.path().join("audit.jsonl");
    let context = context_with_audit_path(audit_path.clone());
    let (frame, plan) = plan_for_command(
        &context,
        &workspace,
        "git -C /aegis-471-no-such-dir filter-branch --index-filter true",
    )
    .await;
    let prompted = Cell::new(false);

    run_watch_plan_with_prompts(
        frame,
        &context,
        plan,
        |_, _| PromptDecision::Approve,
        |_| {
            prompted.set(true);
            RecoveryPromptDecision::Deny
        },
    )
    .await;

    assert!(!prompted.get(), "a human-approved run is not unattended");
    let entry = read_audit_entry(&audit_path);
    assert_eq!(entry["decision"], "Approved");
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_audit_records_the_resolved_cwd_when_the_frame_has_none() {
    let workspace = TempDir::new().unwrap();
    let audit_dir = TempDir::new().unwrap();
    let audit_path = audit_dir.path().join("audit.jsonl");
    fs::write(workspace.path().join("run.sh"), "printf ran > executed\n").unwrap();
    let context = context_with_audit_path(audit_path.clone());
    let (mut frame, plan) = effect_opaque_plan(&context, &workspace).await;
    frame.cwd = None;

    run_watch_plan_with_prompts(
        frame,
        &context,
        plan,
        |_, _| PromptDecision::Approve,
        |_| RecoveryPromptDecision::Deny,
    )
    .await;

    let entry = read_audit_entry(&audit_path);
    assert_eq!(entry["cwd"], workspace.path().to_string_lossy().as_ref());
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_denied_entry_records_the_resolved_cwd_when_the_frame_has_none() {
    let workspace = TempDir::new().unwrap();
    let audit_dir = TempDir::new().unwrap();
    let audit_path = audit_dir.path().join("audit.jsonl");
    let context = context_with_audit_path(audit_path.clone());
    let (mut frame, plan) = plan_for_command(&context, &workspace, "rm -rf build").await;
    frame.cwd = None;

    run_watch_plan_with_prompts(
        frame,
        &context,
        plan,
        |_, _| PromptDecision::Deny,
        |_| RecoveryPromptDecision::Deny,
    )
    .await;

    let entry = read_audit_entry(&audit_path);
    assert_eq!(entry["cwd"], workspace.path().to_string_lossy().as_ref());
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_blocked_command_is_audited_as_blocked_with_no_snapshots() {
    let workspace = TempDir::new().unwrap();
    let audit_dir = TempDir::new().unwrap();
    let audit_path = audit_dir.path().join("audit.jsonl");
    let context = context_with_audit_path(audit_path.clone());
    let (frame, plan) = plan_for_command(&context, &workspace, "rm -rf /").await;

    run_watch_plan_with_prompts(
        frame,
        &context,
        plan,
        |_, _| panic!("a blocked command is never confirmed"),
        |_| panic!("a blocked command takes no Snapshot"),
    )
    .await;

    let entry = read_audit_entry(&audit_path);
    assert_eq!(entry["decision"], "Blocked");
    assert_eq!(entry["snapshots"], serde_json::json!([]));
}

#[test]
fn audit_cwd_makes_a_relative_path_absolute() {
    let recorded = audit_cwd(std::path::Path::new("sub")).unwrap();

    assert_eq!(
        PathBuf::from(recorded),
        std::env::current_dir().unwrap().join("sub")
    );
}

const TERRAFORM_DESTROY: &str = "terraform destroy -target=module.test.api";

fn context_with_allowlisted_danger(
    workspace: &TempDir,
    audit_path: PathBuf,
    plugins: Vec<Box<dyn aegis_snapshot::SnapshotPlugin>>,
) -> RuntimeContext {
    let mut config = AegisConfig::default();
    config.allowlist_override_level = aegis_types::AllowlistOverrideLevel::Danger;
    config.allowlist.push(aegis_config::AllowlistRule {
        pattern: "terraform destroy -target=module.test.*".to_string(),
        cwd: Some(workspace.path().to_string_lossy().into_owned()),
        user: None,
        expires_at: None,
        reason: "test allowlist".to_string(),
    });
    let context =
        RuntimeContext::new_with_audit_path(config, tokio::runtime::Handle::current(), audit_path)
            .unwrap();
    context.set_snapshot_registry_for_tests(SnapshotRegistry::new_with_plugins(plugins));
    context
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_allowlisted_danger_without_a_snapshot_provider_halts_with_no_snapshot_available() {
    let workspace = TempDir::new().unwrap();
    let audit_dir = TempDir::new().unwrap();
    let audit_path = audit_dir.path().join("audit.jsonl");
    let context = context_with_allowlisted_danger(&workspace, audit_path.clone(), vec![]);
    let (frame, plan) = plan_for_command(&context, &workspace, TERRAFORM_DESTROY).await;
    let prompted: Cell<Option<aegis_types::RecoveryDegradation>> = Cell::new(None);

    run_watch_plan_with_prompts(
        frame,
        &context,
        plan,
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
    let entry = read_audit_entry(&audit_path);
    assert_eq!(entry["decision"], "Denied");
    assert_eq!(entry["recovery_degradation"], "no_snapshot_available");
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_allowlisted_danger_with_a_ready_snapshot_runs_without_a_recovery_prompt() {
    let workspace = TempDir::new().unwrap();
    let audit_dir = TempDir::new().unwrap();
    let audit_path = audit_dir.path().join("audit.jsonl");
    let context = context_with_allowlisted_danger(
        &workspace,
        audit_path.clone(),
        vec![Box::new(SucceedingPlugin)],
    );
    let (frame, plan) = plan_for_command(&context, &workspace, TERRAFORM_DESTROY).await;
    let prompted = Cell::new(false);

    run_watch_plan_with_prompts(
        frame,
        &context,
        plan,
        |_, _| PromptDecision::Approve,
        |_| {
            prompted.set(true);
            RecoveryPromptDecision::Deny
        },
    )
    .await;

    assert!(!prompted.get());
    let entry = read_audit_entry(&audit_path);
    assert_eq!(entry["decision"], "AutoApproved");
    assert!(entry.get("recovery_degradation").is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_confirmed_danger_without_a_snapshot_provider_skips_the_recovery_prompt() {
    let workspace = TempDir::new().unwrap();
    let audit_dir = TempDir::new().unwrap();
    let audit_path = audit_dir.path().join("audit.jsonl");
    let context = context_with_default_config(audit_path.clone(), vec![]);
    let (frame, plan) = plan_for_command(&context, &workspace, TERRAFORM_DESTROY).await;
    let prompted = Cell::new(false);

    run_watch_plan_with_prompts(
        frame,
        &context,
        plan,
        |_, _| PromptDecision::Approve,
        |_| {
            prompted.set(true);
            RecoveryPromptDecision::Deny
        },
    )
    .await;

    assert!(
        !prompted.get(),
        "a human-confirmed Danger is not unattended"
    );
    let entry = read_audit_entry(&audit_path);
    assert_eq!(entry["decision"], "Approved");
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_confirmed_danger_with_a_failed_snapshot_skips_the_recovery_prompt() {
    let workspace = TempDir::new().unwrap();
    let audit_dir = TempDir::new().unwrap();
    let audit_path = audit_dir.path().join("audit.jsonl");
    let context = context_with_default_config(audit_path.clone(), vec![Box::new(FailingPlugin)]);
    let (frame, plan) = plan_for_command(&context, &workspace, TERRAFORM_DESTROY).await;
    let prompted = Cell::new(false);

    run_watch_plan_with_prompts(
        frame,
        &context,
        plan,
        |_, _| PromptDecision::Approve,
        |_| {
            prompted.set(true);
            RecoveryPromptDecision::Deny
        },
    )
    .await;

    assert!(
        !prompted.get(),
        "a confirmed Danger stays best-effort when its Snapshot fails"
    );
    let entry = read_audit_entry(&audit_path);
    assert_eq!(entry["decision"], "Approved");
    assert!(entry.get("recovery_degradation").is_none());
}

fn context_with_default_config(
    audit_path: PathBuf,
    plugins: Vec<Box<dyn aegis_snapshot::SnapshotPlugin>>,
) -> RuntimeContext {
    let context = RuntimeContext::new_with_audit_path(
        AegisConfig::default(),
        tokio::runtime::Handle::current(),
        audit_path,
    )
    .unwrap();
    context.set_snapshot_registry_for_tests(SnapshotRegistry::new_with_plugins(plugins));
    context
}

fn audited_attempts(entry: &Value) -> Vec<(String, String)> {
    entry["snapshot_attempts"]
        .as_array()
        .expect("snapshot_attempts")
        .iter()
        .map(|a| {
            (
                a["plugin"].as_str().unwrap().to_string(),
                a["readiness"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn partial_pass_attempts() -> Vec<(String, String)> {
    vec![
        ("mock-succeeding".to_string(), "ready".to_string()),
        ("mock-failing".to_string(), "unavailable".to_string()),
    ]
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_recovery_deny_audits_the_attempts_of_its_snapshot_pass() {
    let workspace = TempDir::new().unwrap();
    let audit_dir = TempDir::new().unwrap();
    let audit_path = audit_dir.path().join("audit.jsonl");
    let context = context_with_partial_coverage(audit_path.clone());
    let (frame, plan) = effect_opaque_plan(&context, &workspace).await;

    run_watch_plan_with_prompts(
        frame,
        &context,
        plan,
        |_, _| PromptDecision::Approve,
        |_| RecoveryPromptDecision::Deny,
    )
    .await;

    let entry = read_audit_entry(&audit_path);
    assert_eq!(audited_attempts(&entry), partial_pass_attempts());
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_recovery_run_once_audits_the_attempts_of_its_snapshot_pass() {
    let workspace = TempDir::new().unwrap();
    let audit_dir = TempDir::new().unwrap();
    let audit_path = audit_dir.path().join("audit.jsonl");
    fs::write(workspace.path().join("run.sh"), "true\n").unwrap();
    let context = context_with_partial_coverage(audit_path.clone());
    let (frame, plan) = effect_opaque_plan(&context, &workspace).await;

    run_watch_plan_with_prompts(
        frame,
        &context,
        plan,
        |_, _| PromptDecision::Approve,
        |_| RecoveryPromptDecision::RunOnceWithoutRecovery,
    )
    .await;

    let entry = read_audit_entry(&audit_path);
    assert_eq!(audited_attempts(&entry), partial_pass_attempts());
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_approved_run_without_recovery_obligation_audits_the_attempts_of_its_snapshot_pass() {
    let workspace = TempDir::new().unwrap();
    let audit_dir = TempDir::new().unwrap();
    let audit_path = audit_dir.path().join("audit.jsonl");
    let context = context_with_partial_coverage(audit_path.clone());
    let (frame, plan) = plan_for_command(&context, &workspace, "rm -rf aegis-557-sentinel").await;

    run_watch_plan_with_prompts(
        frame,
        &context,
        plan,
        |_, _| PromptDecision::Approve,
        |_| panic!("a non-effect-opaque command has no recovery obligation"),
    )
    .await;

    let entry = read_audit_entry(&audit_path);
    assert_eq!(audited_attempts(&entry), partial_pass_attempts());
}

mod approve_always;

mod unavailable_cwd;
