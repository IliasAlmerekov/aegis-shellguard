//! Field-group merge for `PruneConfig`. `enabled` is `Tighten` (#268);
//! `max_count_per_provider`/`max_age_days` are `Custom` — a larger value
//! keeps more Snapshots, but an unset base limit must stay unset rather than
//! adopt whatever the project requests (with both limits unset, prune deletes
//! nothing; letting a project set either one widens deletion).

use std::fmt::Display;

use crate::allowlist::ConfigSourceLayer;

use super::super::PruneConfig;
use super::super::partial::PartialPruneConfig;
use super::direction::{Ratchet, Tighten, bool_false_is_stricter, format_display};
use super::warning::{RatchetSink, is_project};

/// Ratchet a prune retention limit (`prune.max_count_per_provider`,
/// `prune.max_age_days`) where a larger value keeps more Snapshots.
///
/// Global config remains last-wins. A project may raise a limit the base
/// already sets, but cannot lower it or set a limit the base leaves unset:
/// with both limits unset prune deletes nothing, so adding one widens deletion.
fn ratchet_prune_retention<T: Ord + Copy>(
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

fn custom_prune_limit<T: Ord + Copy + Display>(
    field: &'static str,
    base: Option<T>,
    overlay: Option<T>,
    layer: ConfigSourceLayer,
    location: &str,
    sink: &mut RatchetSink,
) -> Option<T> {
    sink.touch(field);
    let kept = ratchet_prune_retention(base, overlay, layer);
    if is_project(layer)
        && let Some(requested) = overlay
    {
        let kept_str = kept.map_or_else(|| "unset".to_string(), |v| v.to_string());
        sink.warn(field, requested.to_string(), kept_str, location);
    }
    kept
}

/// Merge one layer's `[prune]` overlay into the trusted base. The destructure
/// of both structs is an exhaustive tripwire — a new `PruneConfig` field
/// breaks this until it declares a direction.
pub(crate) fn merge_prune(
    base: PruneConfig,
    overlay: PartialPruneConfig,
    layer: ConfigSourceLayer,
    location: &str,
    sink: &mut RatchetSink,
) -> PruneConfig {
    let PruneConfig {
        enabled: base_enabled,
        max_count_per_provider: base_max_count_per_provider,
        max_age_days: base_max_age_days,
    } = base;
    let PartialPruneConfig {
        enabled: req_enabled,
        max_count_per_provider: req_max_count_per_provider,
        max_age_days: req_max_age_days,
    } = overlay;

    PruneConfig {
        enabled: Tighten::new(bool_false_is_stricter, format_display).merge(
            "prune.enabled",
            base_enabled,
            req_enabled,
            layer,
            location,
            sink,
        ),
        max_count_per_provider: custom_prune_limit(
            "prune.max_count_per_provider",
            base_max_count_per_provider,
            req_max_count_per_provider,
            layer,
            location,
            sink,
        ),
        max_age_days: custom_prune_limit(
            "prune.max_age_days",
            base_max_age_days,
            req_max_age_days,
            layer,
            location,
            sink,
        ),
    }
}
