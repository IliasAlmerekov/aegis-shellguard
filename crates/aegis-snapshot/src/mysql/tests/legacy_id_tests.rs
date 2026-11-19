use super::*;

#[tokio::test]
async fn rollback_rejects_legacy_dump_reference_that_is_not_a_plain_file_name() {
    let temp_dir = TempDir::new().unwrap();
    let plugin = plugin_with_user(&temp_dir, "root");
    let id = format!(
        "{}\t{}",
        snapshot_id::encode_bytes(b"app"),
        snapshot_id::encode_bytes(b"../escape.sql")
    );

    let err = plugin.rollback(&id).await.unwrap_err();

    match err {
        SnapshotError::InvalidSnapshotIdComponent { component, value } => {
            assert_eq!(component, "dump reference");
            assert_eq!(value, "../escape.sql");
        }
        other => panic!("expected InvalidSnapshotIdComponent, got {other:?}"),
    }
}
