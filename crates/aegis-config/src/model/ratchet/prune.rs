use std::fmt::Display;

use crate::allowlist::ConfigSourceLayer;

use super::super::PruneConfig;
use super::super::partial::PartialPruneConfig;
use super::{SecurityRatchetWarning, push_ratchet_warning, ratchet_bool_loosen};

/// Ratchet a prune retention limit (`prune.max_count_per_provider`,
/// `prune.max_age_days`) where a larger value keeps more Snapshots.
///
/// Global config remains last-wins. A project may raise a limit the base
/// already sets, but cannot lower it or set a limit the base leaves unset:
/// with both limits unset prune deletes nothing, so adding one widens deletion.
pub(crate) fn ratchet_prune_retention<T: Ord + Copy>(
    base: Option<T>,
    overlay: Option<T>,
    layer: ConfigSourceLayer,
) -> Option<T> {
    match layer {
        ConfigSourceLayer::Global => overlay.or(base),
        ConfigSourceLayer::Project => {
            base.map(|base| overlay.map_or(base, |requested| requested.max(base)))
        }
    }
}

/// Report every `[prune]` value a project layer requested but the ratchet drops.
pub(super) fn push_prune_ratchet_warnings(
    warnings: &mut Vec<SecurityRatchetWarning>,
    base: &PruneConfig,
    overlay: &PartialPruneConfig,
    location: &str,
) {
    if let Some(requested) = overlay.enabled {
        let kept = ratchet_bool_loosen(base.enabled, Some(requested), ConfigSourceLayer::Project);
        push_ratchet_warning(
            warnings,
            "prune.enabled",
            requested.to_string(),
            kept.to_string(),
            location,
        );
    }
    push_retention_warning(
        warnings,
        "prune.max_count_per_provider",
        base.max_count_per_provider,
        overlay.max_count_per_provider,
        location,
    );
    push_retention_warning(
        warnings,
        "prune.max_age_days",
        base.max_age_days,
        overlay.max_age_days,
        location,
    );
}

fn push_retention_warning<T: Display + Ord + Copy>(
    warnings: &mut Vec<SecurityRatchetWarning>,
    field: &'static str,
    base: Option<T>,
    requested: Option<T>,
    location: &str,
) {
    let Some(requested) = requested else {
        return;
    };
    let kept = ratchet_prune_retention(base, Some(requested), ConfigSourceLayer::Project)
        .map_or_else(|| "unset".to_string(), |kept| kept.to_string());
    push_ratchet_warning(warnings, field, requested.to_string(), kept, location);
}
