//! Rollback under a hostile project `.aegis.toml` loaded through the real
//! layered config loader (#269).

use super::*;

#[tokio::test]
async fn hostile_project_overlay_cannot_disable_rollback_target_match_check() {
    // Load via `AegisConfig::load_for` so the project ratchet is exercised end-to-end (#269).
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = write_phase1_manifest_fixture(&temp_dir, &valid_db_dump_checksum());
    let pg_dump = stub_bin(temp_dir.path(), "pg_dump", "exit 0");
    let restore_ran_marker = temp_dir.path().join("pg_restore.ran");
    let pg_restore = stub_bin(
        temp_dir.path(),
        "pg_restore",
        &format!("touch '{}'\nexit 0", restore_ran_marker.display()),
    );

    let home_dir = TempDir::new().unwrap();
    // aegis-config's GLOBAL_CONFIG_DIR/GLOBAL_CONFIG_FILE are private to that
    // crate's `model` module, so this path is spelled out by hand rather than
    // imported.
    let global_config_dir = home_dir.path().join(".config/aegis");
    fs::create_dir_all(&global_config_dir).unwrap();
    fs::write(
        global_config_dir.join("config.toml"),
        "auto_snapshot_supabase = true\n\
         [supabase_snapshot]\n\
         require_config_target_match_on_rollback = true\n\
         [supabase_snapshot.db]\n\
         database = \"postgres\"\n\
         host = \"trusted-host.internal\"\n\
         port = 5432\n\
         user = \"postgres\"\n",
    )
    .unwrap();

    let workspace_dir = TempDir::new().unwrap();
    fs::write(
        workspace_dir.path().join(".aegis.toml"),
        "[supabase_snapshot]\n\
         require_config_target_match_on_rollback = false\n\
         [supabase_snapshot.db]\n\
         database = \"decoy\"\n\
         host = \"decoy.attacker.example\"\n\
         user = \"decoy_user\"\n",
    )
    .unwrap();

    let effective_config =
        aegis_config::AegisConfig::load_for(workspace_dir.path(), Some(home_dir.path())).unwrap();

    // The ratchet must keep the check on despite the hostile request.
    assert!(
        effective_config
            .supabase_snapshot
            .require_config_target_match_on_rollback,
        "project layer must not be able to disable the rollback target-match check"
    );

    let mut plugin = SupabasePlugin::new(
        effective_config.supabase_snapshot,
        temp_dir.path().join("snapshots"),
    );
    plugin.pg_dump_bin = pg_dump.display().to_string();
    plugin.pg_restore_bin = pg_restore.display().to_string();

    let snapshot_id = SupabasePlugin::build_snapshot_id(&manifest_path);
    let err = plugin.rollback(&snapshot_id).await.unwrap_err();

    assert!(matches!(err, SnapshotError::RollbackTargetMismatch { .. }));
    assert!(
        !restore_ran_marker.exists(),
        "pg_restore must never run once the target-match check fails"
    );
}
