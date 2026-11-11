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
        use std::os::unix::fs::PermissionsExt;
        let marker = home.path().join("runs");
        let git = home.path().join("git");
        std::fs::write(
            &git,
            format!("#!/bin/sh\nprintf 'ran\\n' >> {}\n", marker.display()),
        )
        .unwrap();
        std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755)).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                std::thread::current().name().unwrap(),
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("HOME", home.path())
            .env("AEGIS_REAL_SHELL", "/bin/sh")
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    home.path().display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(std::fs::read_to_string(marker).unwrap(), "ran\n");
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
        let prepared = aegis::planning::PreparedPlanner::Ready(Box::new(context));
        let renderer = aegis_tui::TestRenderer::new();
        for decision in [
            aegis_tui::PromptDecision::ApproveAlways,
            aegis_tui::PromptDecision::DenyAlways,
        ] {
            let PlanningOutcome::Planned(plan) = prepared.plan(PlanningRequest {
                command: "git stash clear",
                cwd_state: CwdState::Unavailable,
                transport: ExecutionTransport::Shell,
                ci_detected: false,
            }) else {
                panic!("expected plan")
            };
            renderer.set_confirmation_decision(decision);
            let exit = super::run_planned_shell_command_with_renderer(
                "git stash clear",
                false,
                &prepared,
                &plan,
                &crate::shell_compat::ShellLaunchOptions::default(),
                &renderer,
            );
            assert_eq!(
                exit,
                if decision == aegis_tui::PromptDecision::ApproveAlways {
                    0
                } else {
                    crate::EXIT_DENIED
                }
            );
        }
        assert_eq!(std::fs::read_to_string(config_path).unwrap(), "");
    }
}

#[test]
fn unavailable_cwd_denies_unattended_shell_without_snapshot_plugin() {
    const CHILD: &str = "AEGIS_TEST_UNATTENDED_SHELL_CHILD";
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
    } else {
        use aegis_tui::ExecutionRendererCall;
        let workspace = tempfile::TempDir::new().unwrap();
        let marker = workspace.path().join("marker");
        std::fs::write(&marker, "keep").unwrap();
        let command = format!("rm -rf {}", marker.display());
        let user = std::process::Command::new("id")
            .arg("-un")
            .output()
            .unwrap();
        let user = String::from_utf8(user.stdout).unwrap();
        let mut config = AegisConfig::default();
        config.allowlist_override_level = aegis_types::AllowlistOverrideLevel::Danger;
        config.allowlist.push(aegis_config::AllowlistRule {
            pattern: command.clone(),
            cwd: None,
            user: Some(user.trim().into()),
            expires_at: None,
            reason: "test".into(),
        });
        let context = RuntimeContext::new(config, test_handle()).unwrap();
        let prepared = aegis::planning::PreparedPlanner::Ready(Box::new(context));
        let PlanningOutcome::Planned(plan) = prepared.plan(PlanningRequest {
            command: &command,
            cwd_state: CwdState::Unavailable,
            transport: ExecutionTransport::Shell,
            ci_detected: false,
        }) else {
            panic!("expected plan")
        };
        assert_eq!(plan.execution_disposition(), ExecutionDisposition::Execute);
        let renderer = aegis_tui::TestRenderer::new();
        let exit = super::run_planned_shell_command_with_renderer(
            &command,
            false,
            &prepared,
            &plan,
            &crate::shell_compat::ShellLaunchOptions::default(),
            &renderer,
        );
        assert_eq!(exit, crate::EXIT_DENIED);
        assert_eq!(
            renderer.calls(),
            vec![ExecutionRendererCall::RecoveryOverride(
                aegis_types::RecoveryDegradation::UnavailableCwd
            )]
        );
        assert!(marker.exists());
        let audit = std::fs::read_to_string(
            std::path::PathBuf::from(std::env::var_os("HOME").unwrap()).join(".aegis/audit.jsonl"),
        )
        .unwrap();
        let entry: serde_json::Value = serde_json::from_str(audit.trim()).unwrap();
        assert_eq!(entry["decision"], "Denied");
        assert_eq!(entry["recovery_degradation"], "unavailable_cwd");
    }
}
