use super::*;
use aegis_types::{SnapshotAttemptReadiness as R, StructuralCheck as S, ValidationLevel as V};

fn triple(outcome: ArtifactReadiness) -> (R, V, S) {
    (
        outcome.readiness,
        outcome.validation_level,
        outcome.structural_check,
    )
}

/// A plugin whose `pg_restore` is `pg_restore_body` (`None`: a missing binary),
/// and the id of a dump it holds with `dump_contents`.
fn setup(
    pg_restore_body: Option<&str>,
    dump_contents: Option<&[u8]>,
) -> (TempDir, PostgresPlugin, String) {
    let temp_dir = TempDir::new().unwrap();
    let mut plugin = plugin_with_user(&temp_dir, "postgres");
    let snaps = temp_dir.path().join("snaps");
    fs::create_dir(&snaps).unwrap();
    let dump = snaps.join("d.dump");
    if let Some(contents) = dump_contents {
        fs::write(&dump, contents).unwrap();
    }
    plugin.pg_restore_bin = match pg_restore_body {
        Some(body) => stub_bin(temp_dir.path(), "pg_restore", body)
            .display()
            .to_string(),
        None => temp_dir.path().join("no-pg_restore").display().to_string(),
    };
    let id = snapshot_id_for("app", "localhost", 5432, "postgres", &dump);
    (temp_dir, plugin, id)
}

#[tokio::test]
async fn a_missing_pg_restore_is_tool_unavailable_not_invalid() {
    let (_dir, plugin, id) = setup(None, Some(b"PGDMP"));

    assert_eq!(
        triple(plugin.readiness(&id).await),
        (R::Ready, V::PresenceOnly, S::ToolUnavailable)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_pg_restore_that_lists_the_dump_is_a_structural_pass() {
    let (_dir, plugin, id) = setup(Some("exit 0"), Some(b"PGDMP"));

    assert_eq!(
        triple(plugin.readiness(&id).await),
        (R::Ready, V::Structural, S::Passed)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_pg_restore_that_reports_corruption_is_invalid() {
    let (_dir, plugin, id) = setup(Some("exit 1"), Some(b"not a dump"));

    assert_eq!(
        triple(plugin.readiness(&id).await),
        (R::Invalid, V::PresenceOnly, S::Passed)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_slow_pg_restore_is_timed_out_within_the_attempt_budget() {
    let (_dir, plugin, id) = setup(Some("sleep 5"), Some(b"PGDMP"));

    let started = std::time::Instant::now();
    let outcome = plugin.readiness(&id).await;

    assert_eq!(triple(outcome), (R::Ready, V::PresenceOnly, S::TimedOut));
    assert!(started.elapsed() < crate::readiness::ATTEMPT_TIMEOUT);
}

#[tokio::test]
async fn an_empty_dump_is_invalid_and_not_checked() {
    let (_dir, plugin, id) = setup(None, Some(b""));

    assert_eq!(
        triple(plugin.readiness(&id).await),
        (R::Invalid, V::PresenceOnly, S::NotRun)
    );
}

#[tokio::test]
async fn a_deleted_dump_is_unavailable() {
    let (_dir, plugin, id) = setup(None, None);

    assert_eq!(
        triple(plugin.readiness(&id).await),
        (R::Unavailable, V::PresenceOnly, S::NotRun)
    );
}

#[tokio::test]
async fn a_malformed_id_is_unavailable() {
    let (_dir, plugin, _id) = setup(None, None);

    assert_eq!(
        triple(plugin.readiness("garbage").await),
        (R::Unavailable, V::PresenceOnly, S::NotRun)
    );
}
