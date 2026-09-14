//! `Custom` direction for the Supabase Snapshot target.
//!
//! `PartialSupabaseSnapshotConfig` needs field-level `Option`s, unlike
//! Postgres/MySQL, which stay whole structs, because
//! `require_config_target_match_on_rollback` must ratchet independently of
//! the database-target fields: a project must never disable the rollback
//! target-match check, even for a Supabase target it is otherwise free to
//! configure itself (#269).

use crate::allowlist::ConfigSourceLayer;

use super::super::partial::PartialSupabaseSnapshotConfig;
use super::super::{PostgresSnapshotConfig, SupabaseSnapshotConfig};
use super::direction::bool_true_is_stricter;
use super::warning::{RatchetSink, is_project};

/// Whether a Project-layer overlay must keep every Supabase database-target
/// field pinned to `base`.
fn target_protected(
    base: &SupabaseSnapshotConfig,
    layer: ConfigSourceLayer,
    provider_enabled_in_base: bool,
) -> bool {
    is_project(layer) && provider_enabled_in_base && !base.db.database.is_empty()
}

/// Merge a project/global Supabase overlay into the trusted `base`, and
/// record every per-field weakening attempt. Under the Project layer, once
/// `provider_enabled_in_base` is true AND the base already has a non-empty
/// `db.database`, the database-target fields (`project_ref`, `db.database`,
/// `db.host`, `db.port`, `db.user`) stay pinned to `base` regardless of what
/// the overlay requests. `require_config_target_match_on_rollback` ratchets
/// on its own axis, unconditionally: it is protected even when the project is
/// free to configure its own target. Global stays last-wins for every field.
///
/// The destructure of `overlay.db` is an exhaustive tripwire — a new leaf
/// field on `PostgresSnapshotConfig` breaks this until it is added here.
pub(crate) fn custom_supabase_snapshot(
    base: SupabaseSnapshotConfig,
    overlay: PartialSupabaseSnapshotConfig,
    layer: ConfigSourceLayer,
    location: &str,
    provider_enabled_in_base: bool,
    sink: &mut RatchetSink,
) -> SupabaseSnapshotConfig {
    sink.touch("supabase_snapshot");
    let protected = target_protected(&base, layer, provider_enabled_in_base);
    let PartialSupabaseSnapshotConfig {
        project_ref: req_project_ref,
        require_config_target_match_on_rollback: req_match_on_rollback,
        db: req_db,
    } = overlay;
    let super::super::partial::PartialSupabaseDb {
        database: req_database,
        host: req_host,
        port: req_port,
        user: req_user,
    } = req_db;

    let project_ref = if protected {
        base.project_ref.clone()
    } else {
        req_project_ref
            .clone()
            .unwrap_or_else(|| base.project_ref.clone())
    };
    let db = PostgresSnapshotConfig {
        database: if protected {
            base.db.database.clone()
        } else {
            req_database
                .clone()
                .unwrap_or_else(|| base.db.database.clone())
        },
        host: if protected {
            base.db.host.clone()
        } else {
            req_host.clone().unwrap_or_else(|| base.db.host.clone())
        },
        port: if protected {
            base.db.port
        } else {
            req_port.unwrap_or(base.db.port)
        },
        user: if protected {
            base.db.user.clone()
        } else {
            req_user.clone().unwrap_or_else(|| base.db.user.clone())
        },
    };
    let require_config_target_match_on_rollback = match layer {
        ConfigSourceLayer::Global => {
            req_match_on_rollback.unwrap_or(base.require_config_target_match_on_rollback)
        }
        ConfigSourceLayer::Project => bool_true_is_stricter(
            base.require_config_target_match_on_rollback,
            req_match_on_rollback.unwrap_or(base.require_config_target_match_on_rollback),
        ),
    };

    let kept = SupabaseSnapshotConfig {
        project_ref,
        require_config_target_match_on_rollback,
        db,
    };

    if is_project(layer) {
        if let Some(requested) = req_project_ref {
            sink.warn(
                "supabase_snapshot.project_ref",
                requested,
                kept.project_ref.clone(),
                location,
            );
        }
        if let Some(requested) = req_database {
            sink.warn(
                "supabase_snapshot.db.database",
                requested,
                kept.db.database.clone(),
                location,
            );
        }
        if let Some(requested) = req_host {
            sink.warn(
                "supabase_snapshot.db.host",
                requested,
                kept.db.host.clone(),
                location,
            );
        }
        if let Some(requested) = req_port {
            sink.warn(
                "supabase_snapshot.db.port",
                requested.to_string(),
                kept.db.port.to_string(),
                location,
            );
        }
        if let Some(requested) = req_user {
            sink.warn(
                "supabase_snapshot.db.user",
                requested,
                kept.db.user.clone(),
                location,
            );
        }
        if let Some(requested) = req_match_on_rollback {
            sink.warn(
                "supabase_snapshot.require_config_target_match_on_rollback",
                requested.to_string(),
                kept.require_config_target_match_on_rollback.to_string(),
                location,
            );
        }
    }

    kept
}
