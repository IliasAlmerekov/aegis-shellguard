use crate::allowlist::ConfigSourceLayer;

use super::super::{MysqlSnapshotConfig, PostgresSnapshotConfig};
use super::warning::{RatchetSink, is_project};

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
