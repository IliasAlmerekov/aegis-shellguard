//! Supabase Snapshot-target ratchet: merge and per-field warnings.
//!
//! Lives here rather than in `partial.rs` to match the other three provider
//! ratchets (`ratchet_postgres_snapshot`, `ratchet_mysql_snapshot`,
//! `ratchet_sqlite_path`), which all live in the `ratchet` module.
//! `PartialSupabaseSnapshotConfig` needs field-level `Option`s, unlike
//! Postgres/MySQL, because `require_config_target_match_on_rollback` must
//! ratchet independently of the database-target fields: a project must never
//! disable the rollback target-match check, even for a Supabase target it is
//! otherwise free to configure itself (#269).
//!
//! `push_supabase_ratchet_warnings` derives `kept` by calling
//! [`merge_supabase_snapshot`] — the SAME function `model::merge_layer` calls
//! — so the reported `kept` value always matches what the merge actually
//! produced, the same guarantee the Postgres/MySQL warning helpers give.

use crate::allowlist::ConfigSourceLayer;

use super::super::partial::PartialSupabaseSnapshotConfig;
use super::super::{PostgresSnapshotConfig, SupabaseSnapshotConfig};
use super::{SecurityRatchetWarning, push_ratchet_warning, ratchet_bool_tighten};

/// Whether a Project-layer overlay must keep every Supabase database-target
/// field pinned to `base`. Shared by [`merge_supabase_snapshot`] and
/// [`push_supabase_ratchet_warnings`] so both agree on exactly the same
/// condition (#269).
pub(super) fn supabase_target_protected(
    base: &SupabaseSnapshotConfig,
    source_layer: ConfigSourceLayer,
    provider_enabled_in_base: bool,
) -> bool {
    source_layer == ConfigSourceLayer::Project
        && provider_enabled_in_base
        && !base.db.database.is_empty()
}

/// Merge a project/global Supabase overlay into the trusted `base`.
///
/// Under the Project layer, once `provider_enabled_in_base` is true AND the
/// base already has a non-empty `db.database`, the database-target fields
/// (`project_ref`, `db.database`, `db.host`, `db.port`, `db.user`) stay
/// pinned to `base` regardless of what the overlay requests — a project can
/// no longer repoint an enabled Supabase target at a decoy database.
/// `require_config_target_match_on_rollback` ratchets on its own via
/// `ratchet_bool_tighten`, unconditionally: it is protected even when the
/// project is free to configure its own target. Global stays last-wins for
/// every field.
pub(crate) fn merge_supabase_snapshot(
    base: &SupabaseSnapshotConfig,
    overlay: &PartialSupabaseSnapshotConfig,
    source_layer: ConfigSourceLayer,
    provider_enabled_in_base: bool,
) -> SupabaseSnapshotConfig {
    let target_protected = supabase_target_protected(base, source_layer, provider_enabled_in_base);

    let project_ref = if target_protected {
        base.project_ref.clone()
    } else {
        overlay
            .project_ref
            .clone()
            .unwrap_or_else(|| base.project_ref.clone())
    };
    let db = PostgresSnapshotConfig {
        database: if target_protected {
            base.db.database.clone()
        } else {
            overlay
                .db
                .database
                .clone()
                .unwrap_or_else(|| base.db.database.clone())
        },
        host: if target_protected {
            base.db.host.clone()
        } else {
            overlay
                .db
                .host
                .clone()
                .unwrap_or_else(|| base.db.host.clone())
        },
        port: if target_protected {
            base.db.port
        } else {
            overlay.db.port.unwrap_or(base.db.port)
        },
        user: if target_protected {
            base.db.user.clone()
        } else {
            overlay
                .db
                .user
                .clone()
                .unwrap_or_else(|| base.db.user.clone())
        },
    };

    SupabaseSnapshotConfig {
        project_ref,
        require_config_target_match_on_rollback: ratchet_bool_tighten(
            base.require_config_target_match_on_rollback,
            overlay.require_config_target_match_on_rollback,
            source_layer,
        ),
        db,
    }
}

/// Report every `[supabase_snapshot]` value a project layer requested but the
/// ratchet dropped. `kept` is the SAME [`merge_supabase_snapshot`] result the
/// merge path produces, compared field-by-field against what the overlay
/// requested, so a reported diff always matches the effective merged value.
pub(super) fn push_supabase_ratchet_warnings(
    warnings: &mut Vec<SecurityRatchetWarning>,
    base: &SupabaseSnapshotConfig,
    overlay: &PartialSupabaseSnapshotConfig,
    provider_enabled_in_base: bool,
    location: &str,
) {
    let kept = merge_supabase_snapshot(
        base,
        overlay,
        ConfigSourceLayer::Project,
        provider_enabled_in_base,
    );

    push_string_field_warning(
        warnings,
        "supabase_snapshot.project_ref",
        overlay.project_ref.as_deref(),
        &kept.project_ref,
        location,
    );
    push_string_field_warning(
        warnings,
        "supabase_snapshot.db.database",
        overlay.db.database.as_deref(),
        &kept.db.database,
        location,
    );
    push_string_field_warning(
        warnings,
        "supabase_snapshot.db.host",
        overlay.db.host.as_deref(),
        &kept.db.host,
        location,
    );
    if let Some(requested) = overlay.db.port
        && requested != kept.db.port
    {
        push_ratchet_warning(
            warnings,
            "supabase_snapshot.db.port",
            requested.to_string(),
            kept.db.port.to_string(),
            location,
        );
    }
    push_string_field_warning(
        warnings,
        "supabase_snapshot.db.user",
        overlay.db.user.as_deref(),
        &kept.db.user,
        location,
    );

    if let Some(requested) = overlay.require_config_target_match_on_rollback
        && requested != kept.require_config_target_match_on_rollback
    {
        push_ratchet_warning(
            warnings,
            "supabase_snapshot.require_config_target_match_on_rollback",
            requested.to_string(),
            kept.require_config_target_match_on_rollback.to_string(),
            location,
        );
    }
}

fn push_string_field_warning(
    warnings: &mut Vec<SecurityRatchetWarning>,
    field: &'static str,
    requested: Option<&str>,
    kept: &str,
    location: &str,
) {
    if let Some(requested) = requested
        && requested != kept
    {
        push_ratchet_warning(
            warnings,
            field,
            requested.to_string(),
            kept.to_string(),
            location,
        );
    }
}
