use super::*;
use aegis_types::{SnapshotAttemptReadiness as R, StructuralCheck as S, ValidationLevel as V};

async fn readiness_of_copy(contents: Option<&[u8]>) -> (R, V, S) {
    let temp_dir = TempDir::new().unwrap();
    let snaps = temp_dir.path().join("snaps");
    fs::create_dir(&snaps).unwrap();
    let dump = snaps.join("d.db");
    if let Some(contents) = contents {
        write_db(&dump, contents);
    }
    let plugin = SqlitePlugin::new(PathBuf::from("/tmp/app.db"), snaps);
    let id = SqlitePlugin::build_snapshot_id(Path::new("/tmp/app.db"), &dump);
    let outcome = plugin.readiness(&id).await;
    (
        outcome.readiness,
        outcome.validation_level,
        outcome.structural_check,
    )
}

#[tokio::test]
async fn a_copy_with_the_sqlite_header_is_a_structural_pass() {
    let mut bytes = b"SQLite format 3\0".to_vec();
    bytes.extend_from_slice(&[0; 100]);

    assert_eq!(
        readiness_of_copy(Some(&bytes)).await,
        (R::Ready, V::Structural, S::Passed)
    );
}

#[tokio::test]
async fn a_copy_with_a_wrong_header_is_invalid() {
    assert_eq!(
        readiness_of_copy(Some(b"this is not a sqlite database")).await,
        (R::Invalid, V::PresenceOnly, S::Passed)
    );
}

#[tokio::test]
async fn a_copy_shorter_than_the_header_is_invalid() {
    assert_eq!(
        readiness_of_copy(Some(b"SQLite")).await,
        (R::Invalid, V::PresenceOnly, S::Passed)
    );
}

#[tokio::test]
async fn an_empty_copy_is_invalid_and_not_checked() {
    assert_eq!(
        readiness_of_copy(Some(b"")).await,
        (R::Invalid, V::PresenceOnly, S::NotRun)
    );
}

#[tokio::test]
async fn a_deleted_copy_is_unavailable() {
    assert_eq!(
        readiness_of_copy(None).await,
        (R::Unavailable, V::PresenceOnly, S::NotRun)
    );
}
