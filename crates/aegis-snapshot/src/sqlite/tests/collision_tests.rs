use super::*;

#[tokio::test]
async fn snapshot_never_overwrites_an_existing_artifact_with_the_preferred_name() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("app.db");
    let snapshots_dir = temp_dir.path().join("snaps");
    write_db(&db_path, b"current-database");
    fs::create_dir(&snapshots_dir).unwrap();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let sentinels: Vec<_> = (timestamp..=timestamp + 1)
        .map(|second| {
            let path = snapshots_dir.join(format!("sqlite-app-{second}.db"));
            let contents = format!("existing-{second}");
            fs::write(&path, &contents).unwrap();
            (path, contents)
        })
        .collect();
    let plugin = SqlitePlugin::new(db_path, snapshots_dir);

    let id = plugin
        .snapshot(temp_dir.path(), "sqlite-command")
        .await
        .unwrap();
    let (_, artifact) = SqlitePlugin::parse_snapshot_id(&id).unwrap();

    for (path, contents) in sentinels {
        assert_ne!(artifact, path.canonicalize().unwrap());
        assert_eq!(fs::read(path).unwrap(), contents.as_bytes());
    }
    assert_eq!(fs::read(&artifact).unwrap(), b"current-database");
    assert!(
        artifact
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("-1.db")
    );
}
