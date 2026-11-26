use std::fmt::Display;

use crate::allowlist::ConfigSourceLayer;

use super::super::PruneConfig;
use super::super::partial::PartialPruneConfig;
use super::direction::{Ratchet, Tighten, bool_false_is_stricter, format_display};
use super::warning::{RatchetSink, is_project};

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
