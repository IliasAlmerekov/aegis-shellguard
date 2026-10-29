use super::*;
use aegis_types::{SnapshotAttemptReadiness as R, StructuralCheck as S, ValidationLevel as V};

async fn triple(plugin: &SupabasePlugin, id: &str) -> (R, V, S) {
    let outcome = plugin.readiness(id).await;
    (
        outcome.readiness,
        outcome.validation_level,
        outcome.structural_check,
    )
}

fn plugin_with_pg_restore(temp_dir: &TempDir, pg_restore: Option<&str>) -> SupabasePlugin {
    let mut plugin = SupabasePlugin::new(
        configured_supabase_snapshot_config(
            "proj_123",
            "postgres",
            "db.supabase.co",
            5432,
            "postgres",
        ),
        temp_dir.path().join("snapshots"),
    );
    plugin.pg_restore_bin = match pg_restore {
        Some(body) => stub_bin(temp_dir.path(), "pg_restore", body)
            .display()
            .to_string(),
        None => temp_dir.path().join("no-pg_restore").display().to_string(),
    };
    plugin
}

#[tokio::test]
async fn a_missing_pg_restore_is_tool_unavailable() {
    let temp_dir = TempDir::new().unwrap();
    let manifest = write_phase1_manifest_fixture(&temp_dir, &valid_db_dump_checksum());
    let id = SupabasePlugin::build_snapshot_id(&manifest);

    assert_eq!(
        triple(&plugin_with_pg_restore(&temp_dir, None), &id).await,
        (R::Ready, V::PresenceOnly, S::ToolUnavailable)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_pg_restore_that_lists_the_dump_is_a_structural_pass() {
    let temp_dir = TempDir::new().unwrap();
    let manifest = write_phase1_manifest_fixture(&temp_dir, &valid_db_dump_checksum());
    let id = SupabasePlugin::build_snapshot_id(&manifest);

    assert_eq!(
        triple(&plugin_with_pg_restore(&temp_dir, Some("exit 0")), &id).await,
        (R::Ready, V::Structural, S::Passed)
    );
}

#[tokio::test]
async fn a_deleted_dump_is_unavailable() {
    let temp_dir = TempDir::new().unwrap();
    let manifest = write_phase1_manifest_fixture(&temp_dir, &valid_db_dump_checksum());
    fs::remove_file(manifest.parent().unwrap().join("artifacts/db.dump")).unwrap();
    let id = SupabasePlugin::build_snapshot_id(&manifest);

    assert_eq!(
        triple(&plugin_with_pg_restore(&temp_dir, None), &id).await,
        (R::Unavailable, V::PresenceOnly, S::NotRun)
    );
}

#[tokio::test]
async fn a_missing_manifest_is_unavailable() {
    let temp_dir = TempDir::new().unwrap();
    let id = SupabasePlugin::build_snapshot_id(&manifest_path(&temp_dir));

    assert_eq!(
        triple(&plugin_with_pg_restore(&temp_dir, None), &id).await,
        (R::Unavailable, V::PresenceOnly, S::NotRun)
    );
}
