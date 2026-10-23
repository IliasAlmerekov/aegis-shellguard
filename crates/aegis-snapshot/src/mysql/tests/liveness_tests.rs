use super::*;

fn unreachable_id(dump_path: &Path) -> String {
    snapshot_id_for("app", "10.255.255.1", 1, "postgres", dump_path)
}

#[tokio::test]
async fn liveness_is_present_from_the_local_dump_without_contacting_the_server() {
    let temp_dir = TempDir::new().unwrap();
    let plugin = plugin_with_user(&temp_dir, "postgres");
    let snaps = temp_dir.path().join("snaps");
    fs::create_dir(&snaps).unwrap();
    let dump = snaps.join("d.dump");
    fs::write(&dump, b"x").unwrap();

    let started = std::time::Instant::now();
    let outcome = plugin.liveness(&unreachable_id(&dump)).await.unwrap();

    assert_eq!(outcome, Liveness::Present);
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
}

#[tokio::test]
async fn liveness_is_absent_when_the_dump_is_missing_without_contacting_the_server() {
    let temp_dir = TempDir::new().unwrap();
    let plugin = plugin_with_user(&temp_dir, "postgres");
    let snaps = temp_dir.path().join("snaps");
    fs::create_dir(&snaps).unwrap();

    let started = std::time::Instant::now();
    let outcome = plugin
        .liveness(&unreachable_id(&snaps.join("gone.dump")))
        .await
        .unwrap();

    assert!(matches!(outcome, Liveness::Absent { .. }), "{outcome:?}");
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
}

#[tokio::test]
async fn liveness_rejects_a_dump_outside_the_snapshot_store() {
    let temp_dir = TempDir::new().unwrap();
    let plugin = plugin_with_user(&temp_dir, "postgres");
    fs::create_dir(temp_dir.path().join("snaps")).unwrap();
    let outside = temp_dir.path().join("outside.dump");
    fs::write(&outside, b"x").unwrap();

    let error = plugin
        .liveness(&unreachable_id(&outside))
        .await
        .unwrap_err();

    assert!(
        matches!(error, SnapshotError::PathEscapesSnapshotStore { .. }),
        "{error:?}"
    );
}

#[tokio::test]
async fn liveness_rejects_a_malformed_id() {
    let temp_dir = TempDir::new().unwrap();
    let plugin = plugin_with_user(&temp_dir, "postgres");

    assert!(plugin.liveness("garbage").await.is_err());
}
