use super::*;

#[test]
fn database_path_reads_the_original_file_from_a_v2_id() {
    let id = SqlitePlugin::build_snapshot_id(Path::new("/repo/app.db"), Path::new("/s/d.db"));

    assert_eq!(
        SqlitePlugin::database_path(&id),
        Some(PathBuf::from("/repo/app.db"))
    );
}

#[test]
fn database_path_reads_the_original_file_from_a_legacy_id() {
    assert_eq!(
        SqlitePlugin::database_path("/repo/app.db\t/s/d.db"),
        Some(PathBuf::from("/repo/app.db"))
    );
}

#[test]
fn database_path_is_none_for_a_malformed_id() {
    assert_eq!(SqlitePlugin::database_path("garbage"), None);
}

#[tokio::test]
async fn a_provider_without_a_git_working_tree_has_no_snapshot_target() {
    let plugin = SqlitePlugin::new(PathBuf::from("app.db"), PathBuf::from("/s"));

    assert_eq!(plugin.snapshot_target(Path::new("/repo")).await, None);
}
