use super::*;

#[tokio::test]
async fn liveness_is_present_when_the_dump_exists() {
    let temp_dir = TempDir::new().unwrap();
    let snaps = temp_dir.path().join("snaps");
    fs::create_dir(&snaps).unwrap();
    let dump = snaps.join("d.db");
    write_db(&dump, b"x");
    let plugin = SqlitePlugin::new(PathBuf::from("/tmp/app.db"), snaps);
    let id = SqlitePlugin::build_snapshot_id(Path::new("/tmp/app.db"), &dump);

    assert_eq!(plugin.liveness(&id).await.unwrap(), Liveness::Present);
}

#[tokio::test]
async fn liveness_is_absent_when_the_dump_is_missing() {
    let temp_dir = TempDir::new().unwrap();
    let snaps = temp_dir.path().join("snaps");
    fs::create_dir(&snaps).unwrap();
    let plugin = SqlitePlugin::new(PathBuf::from("/tmp/app.db"), snaps.clone());
    let id = SqlitePlugin::build_snapshot_id(Path::new("/tmp/app.db"), &snaps.join("gone.db"));

    let outcome = plugin.liveness(&id).await.unwrap();

    assert!(matches!(outcome, Liveness::Absent { .. }), "{outcome:?}");
}

#[tokio::test]
async fn liveness_rejects_a_dump_outside_the_snapshot_store() {
    let temp_dir = TempDir::new().unwrap();
    let snaps = temp_dir.path().join("snaps");
    fs::create_dir(&snaps).unwrap();
    let outside = temp_dir.path().join("outside.db");
    write_db(&outside, b"x");
    let plugin = SqlitePlugin::new(PathBuf::from("/tmp/app.db"), snaps);
    let id = SqlitePlugin::build_snapshot_id(Path::new("/tmp/app.db"), &outside);

    let error = plugin.liveness(&id).await.unwrap_err();

    assert!(
        matches!(error, SnapshotError::PathEscapesSnapshotStore { .. }),
        "{error:?}"
    );
}

#[tokio::test]
async fn liveness_rejects_a_malformed_id() {
    let plugin = SqlitePlugin::new(PathBuf::from("/tmp/app.db"), PathBuf::from("/tmp/snaps"));

    assert!(plugin.liveness("garbage").await.is_err());
}
