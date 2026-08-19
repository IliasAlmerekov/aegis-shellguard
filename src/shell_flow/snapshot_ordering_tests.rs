use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use tokio::runtime::Handle;

use super::test_support::{execute_policy_decision, test_command_explanation};
use aegis::audit::Decision;
use aegis::config::{AegisConfig, AllowlistOverrideLevel, SnapshotPolicy};
use aegis::decision::{ExecutionTransport, PolicyAction, PolicyDecision, PolicyRationale};
use aegis::explanation::CommandExplanation;
use aegis::runtime::RuntimeContext;

fn test_handle() -> Handle {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("test runtime build");
    let handle = rt.handle().clone();
    std::mem::forget(rt);
    handle
}

fn danger_context() -> RuntimeContext {
    let mut config = AegisConfig::default();
    config.snapshot_policy = SnapshotPolicy::Selective;
    config.auto_snapshot_git = true;
    config.auto_snapshot_docker = false;
    config.allowlist_override_level = AllowlistOverrideLevel::Danger;
    RuntimeContext::new(config, test_handle()).expect("runtime context")
}

fn init_git_repo(path: &Path) {
    let init = Command::new("git")
        .arg("init")
        .current_dir(path)
        .output()
        .expect("git init");
    assert!(init.status.success(), "git init failed: {init:?}");

    let commit = Command::new("git")
        .args([
            "-c",
            "user.email=test@aegis.dev",
            "-c",
            "user.name=Aegis Test",
            "commit",
            "--allow-empty",
            "-m",
            "init",
        ])
        .current_dir(path)
        .output()
        .expect("git commit");
    assert!(commit.status.success(), "git commit failed: {commit:?}");
}

fn danger_explanation(
    context: &RuntimeContext,
    assessment: &aegis::interceptor::scanner::Assessment,
    policy_decision: PolicyDecision,
    plugins: &[&'static str],
) -> CommandExplanation {
    test_command_explanation(
        context,
        assessment,
        policy_decision,
        None,
        false,
        ExecutionTransport::Shell,
        plugins,
    )
}

#[test]
fn test_execute_policy_decision_prompt_denied_records_no_snapshots() {
    let dir = TempDir::new().expect("temp dir");
    init_git_repo(dir.path());

    let context = danger_context();
    let assessment = aegis::interceptor::assess("rm -rf /tmp/aegis-denied-target").unwrap();
    assert_eq!(assessment.risk, aegis::interceptor::RiskLevel::Danger);

    let policy_decision = PolicyDecision {
        decision: PolicyAction::Prompt,
        rationale: PolicyRationale::RequiresConfirmation,
        requires_confirmation: true,
        snapshots_required: true,
        confinement_required: false,
        allowlist_effective: false,
    };
    let explanation = danger_explanation(&context, &assessment, policy_decision, &["git"]);

    let (decision, snapshots, _) = execute_policy_decision(
        &context,
        &assessment,
        dir.path(),
        policy_decision,
        &explanation,
        false,
    );

    assert_eq!(decision, Decision::Denied);
    assert!(
        snapshots.is_empty(),
        "denied prompt must not create snapshots, got {snapshots:?}"
    );
}

#[test]
fn test_execute_policy_decision_block_records_no_snapshots() {
    let dir = TempDir::new().expect("temp dir");
    init_git_repo(dir.path());

    let context = danger_context();
    let assessment = aegis::interceptor::assess("rm -rf /").unwrap();
    assert_eq!(assessment.risk, aegis::interceptor::RiskLevel::Block);

    let policy_decision = PolicyDecision {
        decision: PolicyAction::Block,
        rationale: aegis::decision::PolicyRationale::IntrinsicRiskBlock,
        requires_confirmation: false,
        snapshots_required: true,
        confinement_required: false,
        allowlist_effective: false,
    };
    let explanation = danger_explanation(&context, &assessment, policy_decision, &[]);

    let (decision, snapshots, _) = execute_policy_decision(
        &context,
        &assessment,
        dir.path(),
        policy_decision,
        &explanation,
        false,
    );

    assert_eq!(decision, Decision::Blocked);
    assert!(
        snapshots.is_empty(),
        "block decision must not create snapshots, got {snapshots:?}"
    );
}
