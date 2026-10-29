use super::*;
use aegis_types::{SnapshotAttemptReadiness as R, StructuralCheck as S, ValidationLevel as V};

fn triple(outcome: ArtifactReadiness) -> (R, V, S) {
    (
        outcome.readiness,
        outcome.validation_level,
        outcome.structural_check,
    )
}

async fn readiness_of_dump(contents: Option<&[u8]>) -> (R, V, S) {
    let temp_dir = TempDir::new().unwrap();
    let plugin = plugin_with_user(&temp_dir, "root");
    let snaps = temp_dir.path().join("snaps");
    fs::create_dir(&snaps).unwrap();
    let dump = snaps.join("d.sql");
    if let Some(contents) = contents {
        fs::write(&dump, contents).unwrap();
    }
    let id = snapshot_id_for("app", "localhost", 3306, "root", &dump);
    triple(plugin.readiness(&id).await)
}

#[tokio::test]
async fn a_present_dump_has_no_structural_check() {
    assert_eq!(
        readiness_of_dump(Some(b"-- dump")).await,
        (R::Ready, V::PresenceOnly, S::NotSupported)
    );
}

#[tokio::test]
async fn an_empty_dump_is_invalid() {
    assert_eq!(
        readiness_of_dump(Some(b"")).await,
        (R::Invalid, V::PresenceOnly, S::NotRun)
    );
}

#[tokio::test]
async fn a_deleted_dump_is_unavailable() {
    assert_eq!(
        readiness_of_dump(None).await,
        (R::Unavailable, V::PresenceOnly, S::NotRun)
    );
}
