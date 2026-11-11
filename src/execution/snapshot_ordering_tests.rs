use std::path::PathBuf;

use aegis::planning::{
    CwdState, ExecutionDisposition, PlanningOutcome, PlanningRequest, SnapshotPlan,
};
use aegis::runtime::RuntimeContext;
use aegis_config::AegisConfig;
use aegis_policy::ExecutionTransport;
use aegis_types::{Mode, SnapshotPolicy};
use tokio::runtime::Handle;

fn test_handle() -> Handle {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let handle = runtime.handle().clone();
    std::mem::forget(runtime);
    handle
}

#[test]
fn effect_opaque_non_danger_plan_requests_recovery_snapshot() {
    let mut config = AegisConfig::default();
    config.mode = Mode::Protect;
    config.snapshot_policy = SnapshotPolicy::Selective;
    config.auto_snapshot_git = false;
    config.auto_snapshot_docker = false;
    let context = RuntimeContext::new(config, test_handle()).unwrap();

    let outcome = aegis::planning::plan_with_context(
        &context,
        PlanningRequest {
            command: "sh ./cleanup.sh",
            cwd_state: CwdState::Resolved(PathBuf::from(".")),
            transport: ExecutionTransport::Shell,
            ci_detected: false,
        },
    );

    let PlanningOutcome::Planned(plan) = outcome else {
        panic!("effect-opaque command must produce a plan");
    };
    assert_eq!(
        plan.execution_disposition(),
        ExecutionDisposition::RequiresApproval
    );
    assert_eq!(
        plan.snapshot_plan(),
        SnapshotPlan::Required {
            applicable_plugins: Vec::new(),
            git_targets: aegis::planning::GitTargetPlan::default(),
        }
    );
}

#[test]
fn unavailable_cwd_cannot_persist_shell_rules() {
    const CHILD: &str = "AEGIS_TEST_UNAVAILABLE_SHELL_RULE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let home = tempfile::TempDir::new().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                std::thread::current().name().unwrap(),
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("HOME", home.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.lines().any(
            |line| line == "warning: cannot persist allow rule: working directory unavailable"
        ));
        assert!(stderr.lines().any(
            |line| line == "warning: cannot persist block rule: working directory unavailable"
        ));
    } else {
        let workspace = tempfile::TempDir::new().unwrap();
        let config_path = workspace.path().join(".aegis.toml");
        std::fs::write(&config_path, "").unwrap();
        std::env::set_current_dir(workspace.path()).unwrap();
        let context = RuntimeContext::new(AegisConfig::default(), test_handle()).unwrap();
        let PlanningOutcome::Planned(plan) = aegis::planning::plan_with_context(
            &context,
            PlanningRequest {
                command: "git stash clear",
                cwd_state: CwdState::Unavailable,
                transport: ExecutionTransport::Shell,
                ci_detected: false,
            },
        ) else {
            panic!("expected plan")
        };
        super::persist_rule(
            "git stash clear",
            &plan,
            aegis_config::amend::append_allow_rule,
            "allow",
        )
        .unwrap();
        super::persist_rule(
            "git stash clear",
            &plan,
            aegis_config::amend::append_block_rule,
            "block",
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(config_path).unwrap(), "");
    }
}
