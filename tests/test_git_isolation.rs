//! `support::init_git_repo` under an inherited `GIT_DIR`, the variable git
//! exports to hooks in a linked worktree.

mod support;

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

const CHILD_REPO: &str = "AEGIS_TEST_GIT_ISOLATION_REPO";
const TEST_NAME: &str = "init_git_repo_ignores_an_inherited_git_dir";

#[test]
fn init_git_repo_ignores_an_inherited_git_dir() {
    // The test re-runs itself with a hostile environment, since setting
    // GIT_DIR in this process would race every other test in the binary.
    match std::env::var_os(CHILD_REPO) {
        Some(repo) => support::init_git_repo(Path::new(&repo)),
        None => rerun_with_a_decoy_git_dir(),
    }
}

fn rerun_with_a_decoy_git_dir() {
    let repo = TempDir::new().unwrap();
    let decoy = TempDir::new().unwrap();
    let decoy_git_dir = decoy.path().join("decoy.git");
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST_NAME, "--test-threads=1"])
        .env(CHILD_REPO, repo.path())
        .env("GIT_DIR", &decoy_git_dir)
        .env("GIT_WORK_TREE", decoy.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "child run failed: {output:?}");
    assert!(
        !decoy_git_dir.exists(),
        "git followed the inherited GIT_DIR"
    );
    assert!(repo.path().join(".git").is_dir());
}
