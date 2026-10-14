//! Planning tests for the Git target (#471): real temp git repositories, the
//! real scanner and the real Snapshot registry applicability checks.

use std::path::{Path, PathBuf};
use std::process::Command;

use aegis_config::AegisConfig;
use aegis_policy::ExecutionTransport;
use aegis_types::{Mode, SnapshotPolicy};
use tempfile::TempDir;

use crate::planning::git_target::{GitTarget, GitTargetUnresolved};
use crate::planning::types::{CwdState, PlanningOutcome, SnapshotPlan};
use crate::planning::{PlanningRequest, plan_with_context};
use crate::runtime::RuntimeContext;

fn git_init(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    let status = Command::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(path)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .status()
        .unwrap();
    assert!(status.success());
}

struct Fixture {
    _root: TempDir,
    cwd: PathBuf,
    a: PathBuf,
    b: PathBuf,
}

fn fixture() -> Fixture {
    let root = TempDir::new().unwrap();
    let base = root.path().canonicalize().unwrap();
    let cwd = base.join("cwd");
    let a = base.join("a");
    let b = base.join("b");
    for dir in [&cwd, &a, &b] {
        git_init(dir);
    }
    Fixture {
        _root: root,
        cwd,
        a,
        b,
    }
}

fn plan_for(fx: &Fixture, command: &str, cwd_state: CwdState) -> SnapshotPlan {
    plan_with_git(fx, command, cwd_state, true)
}

fn plan_with_git(
    fx: &Fixture,
    command: &str,
    cwd_state: CwdState,
    auto_snapshot_git: bool,
) -> SnapshotPlan {
    plan_with_mode(fx, command, cwd_state, auto_snapshot_git, Mode::Protect)
}

fn plan_with_mode(
    fx: &Fixture,
    command: &str,
    cwd_state: CwdState,
    auto_snapshot_git: bool,
    mode: Mode,
) -> SnapshotPlan {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut config = AegisConfig::default();
    config.mode = mode;
    config.snapshot_policy = SnapshotPolicy::Selective;
    config.auto_snapshot_git = auto_snapshot_git;
    config.auto_snapshot_docker = false;
    let context = RuntimeContext::new(config, rt.handle().clone()).unwrap();
    let outcome = std::thread::scope(|scope| {
        scope
            .spawn(|| {
                plan_with_context(
                    &context,
                    PlanningRequest {
                        command,
                        cwd_state,
                        transport: ExecutionTransport::Shell,
                        ci_detected: false,
                    },
                )
            })
            .join()
            .unwrap()
    });
    let _ = &fx.cwd;
    let PlanningOutcome::Planned(plan) = outcome else {
        panic!("expected a normal plan");
    };
    plan.snapshot_plan()
}

fn required(plan: SnapshotPlan) -> (Vec<&'static str>, crate::planning::GitTargetPlan) {
    match plan {
        SnapshotPlan::Required {
            applicable_plugins,
            git_targets,
        } => (applicable_plugins, git_targets),
        SnapshotPlan::NotRequired => panic!("expected a required Snapshot plan"),
    }
}

fn resolved(path: &Path) -> GitTarget {
    GitTarget::Resolved(path.to_path_buf())
}

#[test]
fn dash_c_plans_a_git_snapshot_of_the_other_repository_only() {
    let fx = fixture();
    let command = format!(
        "git -C {} filter-branch --index-filter true",
        fx.a.display()
    );
    let (plugins, targets) = required(plan_for(&fx, &command, CwdState::Resolved(fx.cwd.clone())));

    assert_eq!(targets.targets, vec![resolved(&fx.a)]);
    assert!(!targets.process_cwd);
    assert_eq!(plugins, vec!["git"]);
}

#[test]
fn relative_dash_c_resolves_against_the_cwd() {
    let fx = fixture();
    let nested = fx.cwd.join("nested");
    git_init(&nested);
    let (_, targets) = required(plan_for(
        &fx,
        "git -C nested filter-branch --index-filter true",
        CwdState::Resolved(fx.cwd.clone()),
    ));

    assert_eq!(targets.targets, vec![resolved(&nested)]);
}

#[test]
fn work_tree_and_git_dir_plan_the_work_tree() {
    let fx = fixture();
    let command = format!(
        "git --work-tree {a} --git-dir {a}/.git filter-branch --index-filter true",
        a = fx.a.display()
    );
    let (plugins, targets) = required(plan_for(&fx, &command, CwdState::Resolved(fx.cwd.clone())));

    assert_eq!(targets.targets, vec![resolved(&fx.a)]);
    assert_eq!(plugins, vec!["git"]);
}

#[test]
fn dynamic_path_is_unresolved() {
    // `$X` also degrades the command as effect-opaque (ADR-043, orchestrate),
    // so the cwd may still get its ADR-016 recovery Snapshot; only the Git
    // target is asserted here.
    let fx = fixture();
    let (_, targets) = required(plan_for(
        &fx,
        "git -C \"$X\" filter-branch --index-filter true",
        CwdState::Resolved(fx.cwd.clone()),
    ));

    assert_eq!(
        targets.targets,
        vec![GitTarget::Unresolved(GitTargetUnresolved::DynamicPath)]
    );
}

#[test]
fn missing_path_is_unresolved_and_gets_no_substitute_cwd_snapshot() {
    let fx = fixture();
    let (plugins, targets) = required(plan_for(
        &fx,
        "git -C does-not-exist filter-branch --index-filter true",
        CwdState::Resolved(fx.cwd.clone()),
    ));

    assert_eq!(
        targets.targets,
        vec![GitTarget::Unresolved(GitTargetUnresolved::PathNotFound)]
    );
    assert!(!targets.process_cwd);
    assert!(!plugins.contains(&"git"));
}

#[test]
fn two_redirected_stages_plan_both_targets() {
    let fx = fixture();
    let command = format!(
        "git -C {} filter-branch --index-filter true && git -C {} filter-branch --index-filter true",
        fx.a.display(),
        fx.b.display()
    );
    let (_, targets) = required(plan_for(&fx, &command, CwdState::Resolved(fx.cwd.clone())));

    assert_eq!(targets.targets, vec![resolved(&fx.a), resolved(&fx.b)]);
    assert!(!targets.process_cwd);
}

#[test]
fn a_non_git_danger_stage_adds_the_cwd_snapshot() {
    let fx = fixture();
    let command = format!(
        "git -C {} filter-branch --index-filter true; rm -rf x",
        fx.a.display()
    );
    let (plugins, targets) = required(plan_for(&fx, &command, CwdState::Resolved(fx.cwd.clone())));

    assert_eq!(targets.targets, vec![resolved(&fx.a)]);
    assert!(targets.process_cwd);
    assert_eq!(plugins, vec!["git"]);
}

#[test]
fn unlisted_option_with_two_readings_is_ambiguous() {
    let fx = fixture();
    let (_, targets) = required(plan_for(
        &fx,
        "git --newopt -C a filter-branch --index-filter true",
        CwdState::Resolved(fx.cwd.clone()),
    ));

    assert_eq!(
        targets.targets,
        vec![GitTarget::Unresolved(GitTargetUnresolved::AmbiguousOptions)]
    );
}

#[test]
fn relative_dash_c_without_a_cwd_is_unresolved() {
    let fx = fixture();
    let (_, targets) = required(plan_for(
        &fx,
        "git -C a filter-branch --index-filter true",
        CwdState::Unavailable,
    ));

    assert_eq!(
        targets.targets,
        vec![GitTarget::Unresolved(GitTargetUnresolved::CwdUnavailable)]
    );
}

#[test]
fn environment_redirect_is_unresolved() {
    let fx = fixture();
    let (_, targets) = required(plan_for(
        &fx,
        "GIT_DIR=elsewhere git filter-branch --index-filter true",
        CwdState::Resolved(fx.cwd.clone()),
    ));

    assert!(targets.has_unresolved());
}

#[test]
fn disabled_git_snapshot_still_discloses_an_unresolved_git_target() {
    let fx = fixture();
    let (plugins, targets) = required(plan_with_git(
        &fx,
        "git -C \"$REPO\" filter-branch --index-filter true",
        CwdState::Resolved(fx.cwd.clone()),
        false,
    ));

    assert!(targets.has_unresolved());
    assert!(!plugins.contains(&"git"));
}

#[test]
fn disabled_git_snapshot_runs_nothing_git_for_a_resolved_target() {
    let fx = fixture();
    let command = format!(
        "git -C {} filter-branch --index-filter true",
        fx.a.display()
    );
    let plan = plan_with_git(&fx, &command, CwdState::Resolved(fx.cwd.clone()), false);

    // With no provider left, policy may require no Snapshot at all.
    if let SnapshotPlan::Required {
        applicable_plugins, ..
    } = plan
    {
        assert!(!applicable_plugins.contains(&"git"));
    }
}

#[test]
fn resolved_target_that_is_not_a_git_repository_is_unresolved() {
    let fx = fixture();
    let plain = fx.cwd.parent().unwrap().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let command = format!(
        "git -C {} filter-branch --index-filter true",
        plain.display()
    );
    let (plugins, targets) = required(plan_for(&fx, &command, CwdState::Resolved(fx.cwd.clone())));

    assert_eq!(
        targets.targets,
        vec![GitTarget::Unresolved(
            GitTargetUnresolved::NotAGitRepository
        )]
    );
    assert!(!plugins.contains(&"git"));
}

#[test]
fn audit_mode_takes_no_snapshot_for_an_unresolved_git_target() {
    let fx = fixture();
    let plan = plan_with_mode(
        &fx,
        "git -C \"$REPO\" filter-branch --index-filter true",
        CwdState::Resolved(fx.cwd.clone()),
        true,
        Mode::Audit,
    );

    assert!(matches!(plan, SnapshotPlan::NotRequired));
}

#[test]
fn disabled_git_snapshot_still_reports_a_resolved_target_that_is_not_a_repository() {
    let fx = fixture();
    let plain = fx.cwd.parent().unwrap().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let command = format!(
        "git -C {} filter-branch --index-filter true",
        plain.display()
    );
    let (_, targets) = required(plan_with_git(
        &fx,
        &command,
        CwdState::Resolved(fx.cwd.clone()),
        false,
    ));

    assert_eq!(
        targets.targets,
        vec![GitTarget::Unresolved(
            GitTargetUnresolved::NotAGitRepository
        )]
    );
}
