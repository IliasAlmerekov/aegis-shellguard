//! ADR-031 section 4: `ApproveAlways` persists only after Ready Recovery, and
//! `snapshot_policy = None` offers no persistent approval (#559).

use std::cell::RefCell;

use super::super::run_watch_plan_with_persistence;
use super::*;
use aegis_types::SnapshotPolicy;

const DANGER: &str = "terraform destroy -target=module.test.api";
const WARN: &str = "git stash clear";

#[tokio::test(flavor = "multi_thread")]
async fn watch_approve_always_writes_the_active_config() {
    // Config selection reads process-global HOME and cwd; isolate them in a child.
    const CHILD: &str = "AEGIS_TEST_WATCH_PERSISTENCE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let home = TempDir::new().unwrap();
        let cwd = TempDir::new().unwrap();
        fs::write(cwd.path().join(".aegis.toml"), "").unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                std::thread::current().name().unwrap(),
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("HOME", home.path())
            .current_dir(cwd.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

    let workspace = TempDir::new().unwrap();
    let audit = workspace.path().join("audit.jsonl");
    let prepared = prepared_with_snapshot_policy(audit, SnapshotPolicy::Selective, vec![]);
    let (frame, plan) = plan_for_command(&prepared, &workspace, WARN).await;
    run_watch_plan_with_prompts(
        frame,
        &prepared,
        plan,
        false,
        |_, _| PromptDecision::ApproveAlways,
        |_| panic!("Warn requires no Recovery override"),
    )
    .await;

    let config: AegisConfig = toml::from_str(&fs::read_to_string(".aegis.toml").unwrap()).unwrap();
    assert_eq!(config.allowlist.len(), 1);
    assert_eq!(config.allowlist[0].pattern, "git stash clear");
    assert_eq!(
        config.allowlist[0].cwd.as_deref(),
        workspace.path().to_str()
    );
}

fn prepared_with_snapshot_policy(
    audit_path: PathBuf,
    policy: SnapshotPolicy,
    plugins: Vec<Box<dyn aegis_snapshot::SnapshotPlugin>>,
) -> PreparedPlanner {
    let mut config = AegisConfig::default();
    config.snapshot_policy = policy;
    let context =
        RuntimeContext::new_with_audit_path(config, tokio::runtime::Handle::current(), audit_path)
            .unwrap();
    context.set_snapshot_registry_for_tests(SnapshotRegistry::new_with_plugins(plugins));
    PreparedPlanner::Ready(Box::new(context))
}

/// Run `command` with a human who always answers `ApproveAlways`; returns the
/// ordered `override:` and `persist:` events.
async fn run_approve_always(
    prepared: &PreparedPlanner,
    workspace: &TempDir,
    command: &str,
    recovery: RecoveryPromptDecision,
) -> Vec<String> {
    let (frame, plan) = plan_for_command(prepared, workspace, command).await;
    let events = RefCell::new(Vec::new());
    run_watch_plan_with_persistence(
        frame,
        prepared,
        plan,
        false,
        |_, _| PromptDecision::ApproveAlways,
        |degradation| {
            events
                .borrow_mut()
                .push(format!("override:{degradation:?}"));
            recovery
        },
        |cmd, _| events.borrow_mut().push(format!("persist:{cmd}")),
    )
    .await;
    events.into_inner()
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_approve_always_on_danger_with_a_ready_snapshot_writes_the_rule() {
    let workspace = TempDir::new().unwrap();
    let audit = TempDir::new().unwrap().keep().join("audit.jsonl");
    let prepared = prepared_with_snapshot_policy(
        audit.clone(),
        SnapshotPolicy::Selective,
        vec![Box::new(SucceedingPlugin)],
    );

    let events =
        run_approve_always(&prepared, &workspace, DANGER, RecoveryPromptDecision::Deny).await;

    assert_eq!(events, [format!("persist:{DANGER}")]);
    let entry = read_audit_entry(&audit);
    assert_eq!(entry["decision"], "Approved");
    assert!(
        entry["explanation"]["context"]
            .get("recovery_opt_out")
            .is_none()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_approve_always_on_danger_with_a_degraded_snapshot_writes_no_rule_then_shows_the_override()
 {
    let workspace = TempDir::new().unwrap();
    let audit = TempDir::new().unwrap().keep().join("audit.jsonl");
    let prepared = prepared_with_snapshot_policy(
        audit.clone(),
        SnapshotPolicy::Selective,
        vec![Box::new(FailingPlugin)],
    );

    let events =
        run_approve_always(&prepared, &workspace, DANGER, RecoveryPromptDecision::Deny).await;

    assert_eq!(events, ["override:NoSnapshotAvailable"]);
    let entry = read_audit_entry(&audit);
    assert_eq!(entry["decision"], "Denied");
    assert_eq!(entry["recovery_degradation"], "no_snapshot_available");
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_approve_always_on_danger_run_once_without_recovery_still_writes_no_rule() {
    let workspace = TempDir::new().unwrap();
    let audit = TempDir::new().unwrap().keep().join("audit.jsonl");
    let prepared = prepared_with_snapshot_policy(
        audit.clone(),
        SnapshotPolicy::Selective,
        vec![Box::new(FailingPlugin)],
    );

    let events = run_approve_always(
        &prepared,
        &workspace,
        DANGER,
        RecoveryPromptDecision::RunOnceWithoutRecovery,
    )
    .await;

    assert_eq!(events, ["override:NoSnapshotAvailable"]);
    assert_eq!(read_audit_entry(&audit)["decision"], "Approved");
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_approve_always_on_warn_writes_the_rule_with_no_snapshot() {
    let workspace = TempDir::new().unwrap();
    let audit = TempDir::new().unwrap().keep().join("audit.jsonl");
    let prepared = prepared_with_snapshot_policy(audit.clone(), SnapshotPolicy::Selective, vec![]);

    let events =
        run_approve_always(&prepared, &workspace, WARN, RecoveryPromptDecision::Deny).await;

    assert_eq!(events, [format!("persist:{WARN}")]);
    assert_eq!(read_audit_entry(&audit)["snapshots"], serde_json::json!([]));
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_snapshot_policy_none_marks_the_danger_dialog_and_audit_and_writes_no_rule() {
    let workspace = TempDir::new().unwrap();
    let audit = TempDir::new().unwrap().keep().join("audit.jsonl");
    let prepared = prepared_with_snapshot_policy(audit.clone(), SnapshotPolicy::None, vec![]);
    let (frame, plan) = plan_for_command(&prepared, &workspace, DANGER).await;
    let offered = Cell::new(None);
    let persisted = Cell::new(false);

    run_watch_plan_with_persistence(
        frame,
        &prepared,
        plan,
        false,
        |_, explanation| {
            offered.set(Some(explanation.context.recovery_opt_out));
            PromptDecision::ApproveAlways
        },
        |_| RecoveryPromptDecision::Deny,
        |_, _| persisted.set(true),
    )
    .await;

    assert_eq!(offered.get(), Some(true));
    assert!(!persisted.get(), "None must never persist an approval");
    let entry = read_audit_entry(&audit);
    assert_eq!(entry["decision"], "Approved");
    assert_eq!(entry["explanation"]["context"]["recovery_opt_out"], true);
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_snapshot_policy_none_still_persists_an_ordinary_warn_approval() {
    let workspace = TempDir::new().unwrap();
    let audit = TempDir::new().unwrap().keep().join("audit.jsonl");
    let prepared = prepared_with_snapshot_policy(audit.clone(), SnapshotPolicy::None, vec![]);
    let (frame, plan) = plan_for_command(&prepared, &workspace, WARN).await;
    let opted_out = Cell::new(None);
    let persisted = Cell::new(false);

    run_watch_plan_with_persistence(
        frame,
        &prepared,
        plan,
        false,
        |_, explanation| {
            opted_out.set(Some(explanation.context.recovery_opt_out));
            PromptDecision::ApproveAlways
        },
        |_| RecoveryPromptDecision::Deny,
        |_, _| persisted.set(true),
    )
    .await;

    assert_eq!(opted_out.get(), Some(false));
    assert!(persisted.get(), "a plain Warn keeps persistent approval");
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_snapshot_policy_none_marks_a_denied_danger_audit_entry() {
    let workspace = TempDir::new().unwrap();
    let audit = TempDir::new().unwrap().keep().join("audit.jsonl");
    let prepared = prepared_with_snapshot_policy(audit.clone(), SnapshotPolicy::None, vec![]);
    let (frame, plan) = plan_for_command(&prepared, &workspace, DANGER).await;

    run_watch_plan_with_persistence(
        frame,
        &prepared,
        plan,
        false,
        |_, _| PromptDecision::Deny,
        |_| RecoveryPromptDecision::Deny,
        |_, _| {},
    )
    .await;

    let entry = read_audit_entry(&audit);
    assert_eq!(entry["decision"], "Denied");
    assert_eq!(entry["explanation"]["context"]["recovery_opt_out"], true);
}
