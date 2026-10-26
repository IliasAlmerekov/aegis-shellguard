use super::*;

fn fixture(temp_dir: &TempDir) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let root = temp_dir.path().canonicalize().unwrap();
    let sub = root.join("sub");
    let snapshots_dir = root.join("snaps");
    fs::create_dir_all(&sub).unwrap();
    fs::create_dir_all(&snapshots_dir).unwrap();
    let artifact = snapshots_dir.join("captured.db");
    write_db(&artifact, b"captured-database");
    (root, sub, snapshots_dir, artifact)
}

#[test]
fn rollback_from_a_subdirectory_restores_the_database_the_snapshot_was_taken_of() {
    let temp_dir = TempDir::new().unwrap();
    let (root, sub, snapshots_dir, artifact) = fixture(&temp_dir);
    write_db(&root.join("app.db"), b"damaged-database");
    write_db(&sub.join("app.db"), b"unrelated-database");
    let plugin = SqlitePlugin::new(PathBuf::from("app.db"), snapshots_dir);
    let id = SqlitePlugin::build_snapshot_id(&root.join("app.db"), &artifact);

    plugin.rollback_from(&id, &sub).unwrap();

    assert_eq!(fs::read(root.join("app.db")).unwrap(), b"captured-database");
    assert_eq!(fs::read(sub.join("app.db")).unwrap(), b"unrelated-database");
}

#[test]
fn rollback_ignores_an_id_path_that_is_not_the_configured_database() {
    let temp_dir = TempDir::new().unwrap();
    let (root, sub, snapshots_dir, artifact) = fixture(&temp_dir);
    write_db(&root.join("other.db"), b"other-database");
    write_db(&sub.join("app.db"), b"configured-database");
    let plugin = SqlitePlugin::new(PathBuf::from("app.db"), snapshots_dir);
    let id = SqlitePlugin::build_snapshot_id(&root.join("other.db"), &artifact);

    plugin.rollback_from(&id, &sub).unwrap();

    assert_eq!(fs::read(sub.join("app.db")).unwrap(), b"captured-database");
    assert_eq!(fs::read(root.join("other.db")).unwrap(), b"other-database");
}
