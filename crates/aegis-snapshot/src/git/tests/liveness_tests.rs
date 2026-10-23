use super::tests::{DANGEROUS_CMD, commit_file, init_repo};
use super::*;
use std::fs;
use tempfile::TempDir;

async fn dirty_snapshot(dir: &std::path::Path) -> String {
    init_repo(dir).await;
    commit_file(dir, "a.txt", "one").await;
    fs::write(dir.join("a.txt"), "two").unwrap();
    GitPlugin.snapshot(dir, DANGEROUS_CMD).await.unwrap()
}

#[tokio::test]
async fn liveness_is_present_for_a_live_stash_entry() {
    let dir = TempDir::new().unwrap();
    let id = dirty_snapshot(dir.path()).await;

    assert_eq!(GitPlugin.liveness(&id).await.unwrap(), Liveness::Present);
}

#[tokio::test]
async fn liveness_is_absent_after_the_stash_entry_is_dropped() {
    let dir = TempDir::new().unwrap();
    let id = dirty_snapshot(dir.path()).await;
    git_command(dir.path())
        .args(["stash", "drop"])
        .output()
        .await
        .unwrap();

    let outcome = GitPlugin.liveness(&id).await.unwrap();

    assert!(matches!(outcome, Liveness::Absent { .. }), "{outcome:?}");
}

#[tokio::test]
async fn liveness_is_absent_when_the_repo_directory_is_gone() {
    let dir = TempDir::new().unwrap();
    let id = dirty_snapshot(dir.path()).await;
    fs::remove_dir_all(dir.path()).unwrap();

    let outcome = GitPlugin.liveness(&id).await.unwrap();

    assert!(matches!(outcome, Liveness::Absent { .. }), "{outcome:?}");
}

#[tokio::test]
async fn liveness_is_nothing_captured_for_the_clean_sentinel() {
    assert_eq!(
        GitPlugin.liveness("clean").await.unwrap(),
        Liveness::NothingCaptured
    );
}

#[tokio::test]
async fn liveness_rejects_a_malformed_id() {
    assert!(GitPlugin.liveness("no-separator").await.is_err());
}
