//! `Custom` direction for the scalar/whole-struct database provider targets:
//! `sqlite_snapshot_path`, `postgres_snapshot`, `mysql_snapshot`. Supabase
//! has its own module (`supabase.rs`) because
//! `require_config_target_match_on_rollback` ratchets on a separate axis from
//! its database-target fields (#269).

use crate::allowlist::ConfigSourceLayer;

use super::super::{MysqlSnapshotConfig, PostgresSnapshotConfig};
use super::warning::{RatchetSink, is_project};

/// Core ratchet for a provider's target config (`sqlite_snapshot_path`,
/// `postgres_snapshot`, `mysql_snapshot`). Under the Project layer, once the
/// provider is ENABLED in the trusted base AND the base target itself is
/// enabled (non-no-op), the project may not change ANY target field — host,
/// port, user, database, or path all stay pinned to the trusted base, because
/// a project that could repoint even one field (say, only `host`) could still
/// aim a later Rollback at a decoy database (#269). A project overlay is only
/// honored when the base left the provider off or the base target itself is
/// a no-op — then the project is free to enable and configure its own target.
/// Global stays last-wins.
fn ratchet_provider_target<T: Clone>(
    base: &T,
    overlay: Option<&T>,
    layer: ConfigSourceLayer,
    provider_enabled_in_base: bool,
    base_target_enabled: bool,
) -> T {
    match layer {
        ConfigSourceLayer::Global => overlay.cloned().unwrap_or_else(|| base.clone()),
        ConfigSourceLayer::Project => {
            if !provider_enabled_in_base || !base_target_enabled {
                overlay.cloned().unwrap_or_else(|| base.clone())
            } else {
                base.clone()
            }
        }
    }
}

/// Ratchet the SQLite snapshot path. Target enabled = non-empty path.
pub(crate) fn custom_sqlite_snapshot(
    base: String,
    overlay: Option<String>,
    layer: ConfigSourceLayer,
    location: &str,
    provider_enabled_in_base: bool,
    sink: &mut RatchetSink,
) -> String {
    sink.touch("sqlite_snapshot_path");
    let base_target_enabled = !base.is_empty();
    let kept = ratchet_provider_target(
        &base,
        overlay.as_ref(),
        layer,
        provider_enabled_in_base,
        base_target_enabled,
    );
    if is_project(layer)
        && let Some(requested) = overlay
    {
        sink.warn(
            "sqlite_snapshot_path",
            format!("{requested:?}"),
            format!("{kept:?}"),
            location,
        );
    }
    kept
}

/// Ratchet the PostgreSQL snapshot config. Target enabled = non-empty
/// `database`. The destructure below is a compile-time tripwire: a new leaf
/// field on `PostgresSnapshotConfig` breaks this match until it is added to
/// the per-field warning list.
pub(crate) fn custom_postgres_snapshot(
    base: PostgresSnapshotConfig,
    overlay: Option<PostgresSnapshotConfig>,
    layer: ConfigSourceLayer,
    location: &str,
    provider_enabled_in_base: bool,
    sink: &mut RatchetSink,
) -> PostgresSnapshotConfig {
    sink.touch("postgres_snapshot");
    let base_target_enabled = !base.database.is_empty();
    let kept = ratchet_provider_target(
        &base,
        overlay.as_ref(),
        layer,
        provider_enabled_in_base,
        base_target_enabled,
    );
    if is_project(layer)
        && let Some(PostgresSnapshotConfig {
            database,
            host,
            port,
            user,
        }) = overlay
    {
        sink.warn(
            "postgres_snapshot.database",
            database,
            kept.database.clone(),
            location,
        );
        sink.warn("postgres_snapshot.host", host, kept.host.clone(), location);
        sink.warn(
            "postgres_snapshot.port",
            port.to_string(),
            kept.port.to_string(),
            location,
        );
        sink.warn("postgres_snapshot.user", user, kept.user.clone(), location);
    }
    kept
}

/// MySQL counterpart of [`custom_postgres_snapshot`]. Target enabled =
/// non-empty `database`.
pub(crate) fn custom_mysql_snapshot(
    base: MysqlSnapshotConfig,
    overlay: Option<MysqlSnapshotConfig>,
    layer: ConfigSourceLayer,
    location: &str,
    provider_enabled_in_base: bool,
    sink: &mut RatchetSink,
) -> MysqlSnapshotConfig {
    sink.touch("mysql_snapshot");
    let base_target_enabled = !base.database.is_empty();
    let kept = ratchet_provider_target(
        &base,
        overlay.as_ref(),
        layer,
        provider_enabled_in_base,
        base_target_enabled,
    );
    if is_project(layer)
        && let Some(MysqlSnapshotConfig {
            database,
            host,
            port,
            user,
        }) = overlay
    {
        sink.warn(
            "mysql_snapshot.database",
            database,
            kept.database.clone(),
            location,
        );
        sink.warn("mysql_snapshot.host", host, kept.host.clone(), location);
        sink.warn(
            "mysql_snapshot.port",
            port.to_string(),
            kept.port.to_string(),
            location,
        );
        sink.warn("mysql_snapshot.user", user, kept.user.clone(), location);
    }
    kept
}
