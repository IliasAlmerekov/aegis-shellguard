use super::*;

#[test]
fn parse_snapshot_id_rejects_a_path_that_is_not_an_absolute_manifest() {
    for bad in [
        "/tmp/bundle/not-manifest.json",
        "relative/bundle/manifest.json",
        "/tmp/../bundle/manifest.json",
    ] {
        let id = SupabasePlugin::build_snapshot_id(Path::new(bad));
        let err = SupabasePlugin::parse_snapshot_id(&id).unwrap_err();
        match err {
            SnapshotError::InvalidSnapshotIdPath { label, path } => {
                assert_eq!(label, "manifest path");
                assert_eq!(path, Path::new(bad));
            }
            other => panic!("expected InvalidSnapshotIdPath for {bad}, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn snapshot_rejects_empty_database_in_preflight() {
    let temp_dir = TempDir::new().unwrap();
    let plugin = SupabasePlugin::new(
        database_only_supabase_snapshot_config(""),
        temp_dir.path().join("snapshots"),
    );

    let err = plugin
        .snapshot(temp_dir.path(), "terraform destroy")
        .await
        .unwrap_err();

    match err {
        SnapshotError::Required { subject } => {
            assert_eq!(subject, "supabase_snapshot.db.database")
        }
        other => panic!("expected Required, got {other:?}"),
    }
}

#[tokio::test]
async fn snapshot_rejects_missing_pg_dump_in_preflight() {
    let temp_dir = TempDir::new().unwrap();
    let mut plugin = SupabasePlugin::new(
        database_only_supabase_snapshot_config("postgres"),
        temp_dir.path().join("snapshots"),
    );
    plugin.pg_dump_bin = temp_dir
        .path()
        .join("missing-pg_dump")
        .display()
        .to_string();
    plugin.pg_restore_bin = stub_bin(temp_dir.path(), "pg_restore", "exit 0")
        .display()
        .to_string();

    let err = plugin
        .snapshot(temp_dir.path(), "terraform destroy")
        .await
        .unwrap_err();

    match err {
        SnapshotError::RequiredWhen { subject, condition } => {
            assert_eq!(subject, "pg_dump");
            assert_eq!(condition, "for supabase snapshots");
        }
        other => panic!("expected RequiredWhen, got {other:?}"),
    }
}

#[tokio::test]
async fn snapshot_rejects_missing_pg_restore_in_preflight() {
    let temp_dir = TempDir::new().unwrap();
    let mut plugin = SupabasePlugin::new(
        database_only_supabase_snapshot_config("postgres"),
        temp_dir.path().join("snapshots"),
    );
    plugin.pg_dump_bin = stub_bin(temp_dir.path(), "pg_dump", "exit 0")
        .display()
        .to_string();
    plugin.pg_restore_bin = temp_dir
        .path()
        .join("missing-pg_restore")
        .display()
        .to_string();

    let err = plugin
        .snapshot(temp_dir.path(), "terraform destroy")
        .await
        .unwrap_err();

    match err {
        SnapshotError::RequiredWhen { subject, condition } => {
            assert_eq!(subject, "pg_restore");
            assert_eq!(condition, "for strict supabase rollback");
        }
        other => panic!("expected RequiredWhen, got {other:?}"),
    }
}

#[tokio::test]
async fn delete_reports_an_unparseable_manifest_as_delete_failed() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = manifest_path(&temp_dir);
    fs::create_dir_all(manifest_path.parent().unwrap()).unwrap();
    fs::write(&manifest_path, "not json").unwrap();
    let plugin = SupabasePlugin::new(
        database_only_supabase_snapshot_config("postgres"),
        temp_dir.path().join("snapshots"),
    );
    let snapshot_id = SupabasePlugin::build_snapshot_id(&manifest_path);

    let err = plugin.delete(&snapshot_id).await.unwrap_err();

    match err {
        SnapshotError::DeleteFailed { plugin, detail, .. } => {
            assert_eq!(plugin, "supabase");
            assert!(detail.starts_with("failed to parse manifest"));
        }
        other => panic!("expected DeleteFailed, got {other:?}"),
    }
    assert!(manifest_path.exists());
}
