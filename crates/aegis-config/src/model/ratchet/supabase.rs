//! Supabase target-field ratchet warnings.
//!
//! The Supabase merge lives in `partial::PartialSupabaseSnapshotConfig::merge_into`
//! (it needs field-level `Option`s, unlike Postgres/MySQL, because
//! `require_config_target_match_on_rollback` ratchets independently of whether
//! the project is allowed to touch the database target). This module mirrors
//! that merge field-by-field so every dropped value is reported individually
//! with the SAME `kept` value the merge produced (#269).

use crate::allowlist::ConfigSourceLayer;

use super::super::SupabaseSnapshotConfig;
use super::super::partial::{PartialSupabaseSnapshotConfig, supabase_target_protected};
use super::{SecurityRatchetWarning, push_ratchet_warning, ratchet_bool_tighten};

/// Report every `[supabase_snapshot]` value a project layer requested but the
/// ratchet dropped: the database target fields (`project_ref`, `db.database`,
/// `db.host`, `db.port`, `db.user`) when the provider is enabled in the
/// trusted base with a non-empty target, and
/// `require_config_target_match_on_rollback` unconditionally — a project must
/// never be able to switch off the rollback target-match check, even for a
/// Supabase target it enabled itself.
pub(super) fn push_supabase_ratchet_warnings(
    warnings: &mut Vec<SecurityRatchetWarning>,
    base: &SupabaseSnapshotConfig,
    overlay: &PartialSupabaseSnapshotConfig,
    provider_enabled_in_base: bool,
    location: &str,
) {
    let target_protected =
        supabase_target_protected(base, ConfigSourceLayer::Project, provider_enabled_in_base);

    if target_protected {
        push_string_field_warning(
            warnings,
            "supabase_snapshot.project_ref",
            overlay.project_ref.as_deref(),
            &base.project_ref,
            location,
        );
        push_string_field_warning(
            warnings,
            "supabase_snapshot.db.database",
            overlay.db.database.as_deref(),
            &base.db.database,
            location,
        );
        push_string_field_warning(
            warnings,
            "supabase_snapshot.db.host",
            overlay.db.host.as_deref(),
            &base.db.host,
            location,
        );
        if let Some(requested) = overlay.db.port
            && requested != base.db.port
        {
            push_ratchet_warning(
                warnings,
                "supabase_snapshot.db.port",
                requested.to_string(),
                base.db.port.to_string(),
                location,
            );
        }
        push_string_field_warning(
            warnings,
            "supabase_snapshot.db.user",
            overlay.db.user.as_deref(),
            &base.db.user,
            location,
        );
    }

    if let Some(requested) = overlay.require_config_target_match_on_rollback {
        let kept = ratchet_bool_tighten(
            base.require_config_target_match_on_rollback,
            Some(requested),
            ConfigSourceLayer::Project,
        );
        push_ratchet_warning(
            warnings,
            "supabase_snapshot.require_config_target_match_on_rollback",
            requested.to_string(),
            kept.to_string(),
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
