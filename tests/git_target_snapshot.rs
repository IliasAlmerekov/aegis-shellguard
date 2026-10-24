//! #471: the git Snapshot covers the Git target, not the process cwd.
//!
//! Plans a Danger git command with real temp repositories, takes the Snapshot
//! the plan asks for, destroys the target, and restores it through the rollback
//! registry.

use std::path::{Path, PathBuf};
use std::process::Command;

use aegis::planning::{
    CwdState, PlanningOutcome, PlanningRequest, SnapshotPlan, plan_with_context,
};
use aegis::runtime::RuntimeContext;
use aegis_config::AegisConfig;
use aegis_policy::ExecutionTransport;
use aegis_snapshot::SnapshotRegistry;
use aegis_types::{Mode, SnapshotPolicy};
use tempfile::TempDir;

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A repository with one commit holding `tracked.txt`.
fn repo(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    git(path, &["init", "-q"]);
    git(path, &["config", "user.email", "test@aegis.dev"]);
    git(path, &["config", "user.name", "Aegis Test"]);
    git(path, &["config", "core.autocrlf", "false"]);
    std::fs::write(path.join("tracked.txt"), "committed\n").unwrap();
    git(path, &["add", "tracked.txt"]);
    git(path, &["commit", "-q", "-m", "init"]);
}

fn dirty(path: &Path) {
    std::fs::write(path.join("tracked.txt"), "uncommitted work\n").unwrap();
    std::fs::write(path.join("untracked.txt"), "new file\n").unwrap();
}

fn destroy(path: &Path) {
    git(path, &["reset", "--hard", "-q"]);
    git(path, &["clean", "-fdxq"]);
}

struct World {
    _root: TempDir,
    cwd: PathBuf,
    a: PathBuf,
    b: PathBuf,
}

fn world() -> World {
    let root = TempDir::new().unwrap();
    let base = root.path().canonicalize().unwrap();
    let world = World {
        cwd: base.join("cwd"),
        a: base.join("a"),
        b: base.join("b"),
        _root: root,
    };
    for dir in [&world.cwd, &world.a, &world.b] {
        repo(dir);
    }
    world
}

fn context(rt: &tokio::runtime::Runtime) -> RuntimeContext {
    let mut config = AegisConfig::default();
    config.mode = Mode::Protect;
    config.snapshot_policy = SnapshotPolicy::Selective;
    config.auto_snapshot_git = true;
    config.auto_snapshot_docker = false;
    RuntimeContext::new(config, rt.handle().clone()).unwrap()
}

/// Plan `command` from `cwd`, take the planned Snapshots, and return the
/// git records' ids.
fn snapshot(rt: &tokio::runtime::Runtime, cwd: &Path, command: &str) -> Vec<String> {
    git_records(rt, cwd, command)
        .into_iter()
        .map(|record| record.snapshot_id)
        .collect()
}

fn git_records(
    rt: &tokio::runtime::Runtime,
    cwd: &Path,
    command: &str,
) -> Vec<aegis_types::SnapshotRecord> {
    let context = context(rt);
    let outcome = plan_with_context(
        &context,
        PlanningRequest {
            command,
            cwd_state: CwdState::Resolved(cwd.to_path_buf()),
            transport: ExecutionTransport::Shell,
            ci_detected: false,
        },
    );
    let PlanningOutcome::Planned(plan) = outcome else {
        panic!("expected a normal plan");
    };
    let SnapshotPlan::Required { git_targets, .. } = plan.snapshot_plan() else {
        panic!("a Danger git command must require a Snapshot");
    };
    let coverage = context.create_snapshots_for_plan(cwd, command, &git_targets);
    coverage
        .records
        .into_iter()
        .filter(|record| record.plugin == "git")
        .collect()
}

fn rollback(rt: &tokio::runtime::Runtime, id: &str) {
    let registry = SnapshotRegistry::for_rollback().unwrap();
    rt.block_on(registry.rollback("git", id)).unwrap();
}

fn read(path: &Path, name: &str) -> String {
    std::fs::read_to_string(path.join(name)).unwrap()
}

fn assert_restored(path: &Path) {
    assert_eq!(read(path, "tracked.txt"), "uncommitted work\n");
    assert_eq!(read(path, "untracked.txt"), "new file\n");
}

fn assert_untouched(path: &Path) {
    assert_eq!(read(path, "tracked.txt"), "committed\n");
    assert!(!path.join("untracked.txt").exists());
    assert_eq!(git(path, &["status", "--porcelain"]), "");
    assert_eq!(git(path, &["stash", "list"]), "");
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn dash_c_snapshot_restores_the_other_repository_and_leaves_the_cwd_alone() {
    let rt = runtime();
    let world = world();
    dirty(&world.a);
    let command = format!(
        "git -C {} filter-branch --index-filter true",
        world.a.display()
    );

    let ids = snapshot(&rt, &world.cwd, &command);
    assert_eq!(ids.len(), 1);
    destroy(&world.a);
    rollback(&rt, &ids[0]);

    assert_restored(&world.a);
    assert_untouched(&world.cwd);
}

/// `aegis rollback --last` binds a git Snapshot by this record (#499, ADR-053).
#[test]
fn the_git_record_names_its_git_target() {
    let rt = runtime();
    let world = world();
    dirty(&world.a);
    let command = format!(
        "git -C {} filter-branch --index-filter true",
        world.a.display()
    );

    let records = git_records(&rt, &world.cwd, &command);

    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].git_target.as_deref(),
        Some(world.a.canonicalize().unwrap().to_str().unwrap())
    );
}

#[test]
fn work_tree_and_git_dir_snapshot_restores_the_other_repository() {
    let rt = runtime();
    let world = world();
    dirty(&world.a);
    let command = format!(
        "git --work-tree {a} --git-dir {a}/.git filter-branch --index-filter true",
        a = world.a.display()
    );

    let ids = snapshot(&rt, &world.cwd, &command);
    assert_eq!(ids.len(), 1);
    destroy(&world.a);
    rollback(&rt, &ids[0]);

    assert_restored(&world.a);
    assert_untouched(&world.cwd);
}

#[test]
fn two_redirected_stages_snapshot_both_repositories() {
    let rt = runtime();
    let world = world();
    dirty(&world.a);
    dirty(&world.b);
    let command = format!(
        "git -C {} filter-branch --index-filter true && git -C {} filter-branch --index-filter true",
        world.a.display(),
        world.b.display()
    );

    let ids = snapshot(&rt, &world.cwd, &command);
    assert_eq!(ids.len(), 2);
    destroy(&world.a);
    destroy(&world.b);
    for id in &ids {
        rollback(&rt, id);
    }

    assert_restored(&world.a);
    assert_restored(&world.b);
    assert_untouched(&world.cwd);
}

#[test]
fn a_non_git_danger_stage_also_snapshots_the_cwd() {
    let rt = runtime();
    let world = world();
    dirty(&world.a);
    dirty(&world.cwd);
    let command = format!(
        "git -C {} filter-branch --index-filter true; rm -rf x",
        world.a.display()
    );

    let ids = snapshot(&rt, &world.cwd, &command);
    assert_eq!(ids.len(), 2);
    destroy(&world.a);
    destroy(&world.cwd);
    for id in &ids {
        rollback(&rt, id);
    }

    assert_restored(&world.a);
    assert_restored(&world.cwd);
}

#[test]
fn an_unresolved_target_takes_no_substitute_snapshot_of_the_cwd() {
    let rt = runtime();
    let world = world();
    dirty(&world.cwd);

    let ids = snapshot(
        &rt,
        &world.cwd,
        "git -C does-not-exist filter-branch --index-filter true",
    );

    assert!(ids.is_empty());
}

// #525: the Snapshot follows a `cd` earlier in the command line.

#[test]
fn cd_snapshot_restores_the_other_repository_and_leaves_the_cwd_alone() {
    let rt = runtime();
    let world = world();
    dirty(&world.a);

    let ids = snapshot(&rt, &world.cwd, "cd ../a && rm -rf x");
    assert_eq!(ids.len(), 1);
    destroy(&world.a);
    rollback(&rt, &ids[0]);

    assert_restored(&world.a);
    assert_untouched(&world.cwd);
}

#[test]
fn cd_then_git_dash_c_snapshots_the_nested_repository() {
    let rt = runtime();
    let world = world();
    let inner = world.a.join("inner");
    repo(&inner);
    dirty(&inner);

    let ids = snapshot(
        &rt,
        &world.cwd,
        "cd ../a && git -C inner filter-branch --index-filter true",
    );
    assert_eq!(ids.len(), 1);
    destroy(&inner);
    rollback(&rt, &ids[0]);

    assert_restored(&inner);
}

#[test]
fn two_cd_steps_snapshot_both_repositories() {
    let rt = runtime();
    let world = world();
    dirty(&world.a);
    dirty(&world.b);

    let ids = snapshot(
        &rt,
        &world.cwd,
        "cd ../a && rm -rf x && cd ../b && rm -rf y",
    );
    assert_eq!(ids.len(), 2);
    destroy(&world.a);
    destroy(&world.b);
    for id in &ids {
        rollback(&rt, id);
    }

    assert_restored(&world.a);
    assert_restored(&world.b);
    assert_untouched(&world.cwd);
}
