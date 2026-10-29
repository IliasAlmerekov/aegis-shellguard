use super::tests::{DANGEROUS_CMD, commit_file, init_repo};
use super::*;
use aegis_types::{SnapshotAttemptReadiness as R, StructuralCheck as S, ValidationLevel as V};
use std::fs;
use tempfile::TempDir;

fn triple(outcome: ArtifactReadiness) -> (R, V, S) {
    (
        outcome.readiness,
        outcome.validation_level,
        outcome.structural_check,
    )
}

async fn dirty_snapshot(dir: &std::path::Path) -> String {
    init_repo(dir).await;
    commit_file(dir, "a.txt", "one").await;
    fs::write(dir.join("a.txt"), "two").unwrap();
    GitPlugin.snapshot(dir, DANGEROUS_CMD).await.unwrap()
}

#[tokio::test]
async fn a_live_stash_commit_is_present_with_no_structural_check() {
    let dir = TempDir::new().unwrap();
    let id = dirty_snapshot(dir.path()).await;

    assert_eq!(
        triple(GitPlugin.readiness(&id).await),
        (R::Ready, V::PresenceOnly, S::NotSupported)
    );
}

#[tokio::test]
async fn a_stash_commit_that_is_gone_is_unavailable() {
    let dir = TempDir::new().unwrap();
    let id = dirty_snapshot(dir.path()).await;
    let (cwd, _hash) = id.split_once(SEP).unwrap();
    let bogus = format!("{cwd}{SEP}{}", "0".repeat(40));

    assert_eq!(
        triple(GitPlugin.readiness(&bogus).await),
        (R::Unavailable, V::PresenceOnly, S::NotRun)
    );
}

#[tokio::test]
async fn a_repo_directory_that_is_gone_is_unavailable() {
    let dir = TempDir::new().unwrap();
    let id = dirty_snapshot(dir.path()).await;
    fs::remove_dir_all(dir.path()).unwrap();

    assert_eq!(
        triple(GitPlugin.readiness(&id).await),
        (R::Unavailable, V::PresenceOnly, S::NotRun)
    );
}

#[tokio::test]
async fn the_clean_sentinel_met_the_obligation_with_nothing_to_capture() {
    assert_eq!(
        triple(GitPlugin.readiness("clean").await),
        (R::Ready, V::PresenceOnly, S::NotSupported)
    );
}

#[tokio::test]
async fn a_malformed_id_is_unavailable() {
    assert_eq!(
        triple(GitPlugin.readiness("no-separator").await),
        (R::Unavailable, V::PresenceOnly, S::NotRun)
    );
}
