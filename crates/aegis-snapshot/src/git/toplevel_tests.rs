use super::*;
use std::fs;
use tempfile::TempDir;

#[tokio::test]
async fn git_toplevel_of_a_subdirectory_is_the_repository_root() {
    let dir = TempDir::new().unwrap();
    git_command(dir.path())
        .args(["init"])
        .output()
        .await
        .unwrap();
    let sub = dir.path().join("deep/nested");
    fs::create_dir_all(&sub).unwrap();

    assert_eq!(
        git_toplevel(&sub).await,
        Some(dir.path().canonicalize().unwrap())
    );
}

#[tokio::test]
async fn git_toplevel_outside_a_repository_is_none() {
    let dir = TempDir::new().unwrap();

    assert_eq!(git_toplevel(dir.path()).await, None);
}

#[tokio::test]
async fn snapshot_target_is_the_canonical_repository_top_level() {
    let dir = TempDir::new().unwrap();
    git_command(dir.path())
        .args(["init"])
        .output()
        .await
        .unwrap();
    let sub = dir.path().join("a").join("b");
    fs::create_dir_all(&sub).unwrap();

    let target = GitPlugin.snapshot_target(&sub).await;

    assert_eq!(
        target.as_deref(),
        Some(dir.path().canonicalize().unwrap().to_str().unwrap())
    );
}
