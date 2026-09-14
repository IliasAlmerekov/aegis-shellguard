use super::super::{MysqlSnapshotConfig, PostgresSnapshotConfig};
use super::{SecurityRatchetWarning, push_ratchet_warning};

/// Report each PostgreSQL target field a project layer requested but the
/// ratchet dropped, comparing `requested` (the raw overlay) against `kept`
/// (the value `ratchet_postgres_snapshot` actually merged) field-by-field so
/// the reported diffs match the merge exactly.
pub(super) fn push_postgres_target_field_warnings(
    warnings: &mut Vec<SecurityRatchetWarning>,
    requested: &PostgresSnapshotConfig,
    kept: &PostgresSnapshotConfig,
    location: &str,
) {
    if requested.database != kept.database {
        push_ratchet_warning(
            warnings,
            "postgres_snapshot.database",
            requested.database.clone(),
            kept.database.clone(),
            location,
        );
    }
    if requested.host != kept.host {
        push_ratchet_warning(
            warnings,
            "postgres_snapshot.host",
            requested.host.clone(),
            kept.host.clone(),
            location,
        );
    }
    if requested.port != kept.port {
        push_ratchet_warning(
            warnings,
            "postgres_snapshot.port",
            requested.port.to_string(),
            kept.port.to_string(),
            location,
        );
    }
    if requested.user != kept.user {
        push_ratchet_warning(
            warnings,
            "postgres_snapshot.user",
            requested.user.clone(),
            kept.user.clone(),
            location,
        );
    }
}

/// MySQL counterpart of [`push_postgres_target_field_warnings`].
pub(super) fn push_mysql_target_field_warnings(
    warnings: &mut Vec<SecurityRatchetWarning>,
    requested: &MysqlSnapshotConfig,
    kept: &MysqlSnapshotConfig,
    location: &str,
) {
    if requested.database != kept.database {
        push_ratchet_warning(
            warnings,
            "mysql_snapshot.database",
            requested.database.clone(),
            kept.database.clone(),
            location,
        );
    }
    if requested.host != kept.host {
        push_ratchet_warning(
            warnings,
            "mysql_snapshot.host",
            requested.host.clone(),
            kept.host.clone(),
            location,
        );
    }
    if requested.port != kept.port {
        push_ratchet_warning(
            warnings,
            "mysql_snapshot.port",
            requested.port.to_string(),
            kept.port.to_string(),
            location,
        );
    }
    if requested.user != kept.user {
        push_ratchet_warning(
            warnings,
            "mysql_snapshot.user",
            requested.user.clone(),
            kept.user.clone(),
            location,
        );
    }
}
