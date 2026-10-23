use super::*;

fn liveness_plugin(temp_dir: &TempDir) -> SupabasePlugin {
    SupabasePlugin::new(
        configured_supabase_snapshot_config(
            "proj_123",
            "postgres",
            "db.supabase.co",
            5432,
            "postgres",
        ),
        temp_dir.path().join("snapshots"),
    )
}

#[tokio::test]
async fn liveness_is_present_for_an_eligible_manifest_with_its_dump() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = write_phase1_manifest_fixture(&temp_dir, &valid_db_dump_checksum());
    let snapshot_id = SupabasePlugin::build_snapshot_id(&manifest_path);

    let outcome = liveness_plugin(&temp_dir)
        .liveness(&snapshot_id)
        .await
        .unwrap();

    assert_eq!(outcome, Liveness::Present);
}

#[tokio::test]
async fn liveness_is_absent_when_the_manifest_is_missing() {
    let temp_dir = TempDir::new().unwrap();
    let snapshot_id = SupabasePlugin::build_snapshot_id(&manifest_path(&temp_dir));

    let outcome = liveness_plugin(&temp_dir)
        .liveness(&snapshot_id)
        .await
        .unwrap();

    assert!(matches!(outcome, Liveness::Absent { .. }), "{outcome:?}");
}

#[tokio::test]
async fn liveness_is_absent_when_the_dump_is_missing() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = write_phase1_manifest_fixture(&temp_dir, &valid_db_dump_checksum());
    fs::remove_file(manifest_path.parent().unwrap().join("artifacts/db.dump")).unwrap();
    let snapshot_id = SupabasePlugin::build_snapshot_id(&manifest_path);

    let outcome = liveness_plugin(&temp_dir)
        .liveness(&snapshot_id)
        .await
        .unwrap();

    assert!(matches!(outcome, Liveness::Absent { .. }), "{outcome:?}");
}

#[tokio::test]
async fn liveness_is_absent_when_the_artifact_directory_is_gone() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = write_phase1_manifest_fixture(&temp_dir, &valid_db_dump_checksum());
    fs::remove_dir_all(manifest_path.parent().unwrap().join("artifacts")).unwrap();
    let snapshot_id = SupabasePlugin::build_snapshot_id(&manifest_path);

    let outcome = liveness_plugin(&temp_dir)
        .liveness(&snapshot_id)
        .await
        .unwrap();

    assert!(matches!(outcome, Liveness::Absent { .. }), "{outcome:?}");
}

#[tokio::test]
async fn liveness_rejects_a_manifest_that_fails_eligibility() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = write_phase1_manifest_fixture(&temp_dir, &valid_db_dump_checksum());
    let mut manifest: SupabaseManifest =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest.rollback.allowed = false;
    write_manifest_atomically(&manifest_path, &manifest).unwrap();
    let snapshot_id = SupabasePlugin::build_snapshot_id(&manifest_path);

    let result = liveness_plugin(&temp_dir).liveness(&snapshot_id).await;

    assert!(result.is_err(), "{result:?}");
}

#[tokio::test]
async fn liveness_rejects_a_manifest_dump_path_escaping_the_bundle() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = write_phase1_manifest_fixture(&temp_dir, &valid_db_dump_checksum());
    let mut manifest: SupabaseManifest =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest.artifacts.db.path = Some("../outside.dump".to_string());
    write_manifest_atomically(&manifest_path, &manifest).unwrap();
    let snapshot_id = SupabasePlugin::build_snapshot_id(&manifest_path);

    let result = liveness_plugin(&temp_dir).liveness(&snapshot_id).await;

    assert!(result.is_err(), "{result:?}");
}

#[tokio::test]
async fn liveness_rejects_a_malformed_id() {
    let temp_dir = TempDir::new().unwrap();

    assert!(
        liveness_plugin(&temp_dir)
            .liveness("v1\x00invalid")
            .await
            .is_err()
    );
}
