//! Field-group merge for `SandboxSettings`. `enabled`/`required` are
//! `Tighten` (current behaviour — #229 tracks folding the Sandbox's mandatory
//! posture in here directly). `allow_write` is `Custom` (a set that can only
//! narrow, not last-wins). `allow_network` is `Tighten` with `false` as the
//! stricter direction.

use std::path::PathBuf;

use crate::allowlist::ConfigSourceLayer;

use super::super::SandboxSettings;
use super::super::partial::PartialSandboxSettings;
use super::direction::{
    Ratchet, Tighten, bool_false_is_stricter, bool_true_is_stricter, format_display,
};
use super::warning::{RatchetSink, is_project};

/// Ratchet `sandbox.allow_write` (a `Vec<PathBuf>` — more entries = weaker).
///
/// - Global layer: last-wins (`overlay` replaces `base` when present).
/// - Project layer: keep the intersection (`base` filtered to entries present
///   in `overlay`, preserving base order). This honors project tightening to a
///   subset (including the empty set) while preventing any expansion beyond the
///   trusted base.
pub(crate) fn custom_allow_write(
    base: Vec<PathBuf>,
    overlay: Option<Vec<PathBuf>>,
    layer: ConfigSourceLayer,
    location: &str,
    sink: &mut RatchetSink,
) -> Vec<PathBuf> {
    sink.touch("sandbox.allow_write");
    let kept = match layer {
        ConfigSourceLayer::Global => overlay.clone().unwrap_or_else(|| base.clone()),
        ConfigSourceLayer::Project => match &overlay {
            None => base.clone(),
            Some(requested) => base
                .iter()
                .filter(|path| requested.contains(path))
                .cloned()
                .collect(),
        },
    };

    if is_project(layer)
        && let Some(requested) = &overlay
    {
        // Gate on genuine expansion (some requested path is outside the
        // trusted base) rather than a Debug-string inequality, so a
        // reordered-but-equal subset does not spuriously warn.
        let weakened = requested.iter().any(|path| !base.contains(path));
        if weakened {
            sink.warn(
                "sandbox.allow_write",
                format!("{requested:?}"),
                format!("{kept:?}"),
                location,
            );
        }
    }

    kept
}

/// Merge one layer's `[sandbox]` overlay into the trusted base. The
/// destructure of both structs is an exhaustive tripwire — a new
/// `SandboxSettings` field breaks this until it declares a direction.
pub(crate) fn merge_sandbox(
    base: SandboxSettings,
    overlay: PartialSandboxSettings,
    layer: ConfigSourceLayer,
    location: &str,
    sink: &mut RatchetSink,
) -> SandboxSettings {
    let SandboxSettings {
        enabled: base_enabled,
        required: base_required,
        allow_write: base_allow_write,
        allow_network: base_allow_network,
    } = base;
    let PartialSandboxSettings {
        enabled: req_enabled,
        required: req_required,
        allow_write: req_allow_write,
        allow_network: req_allow_network,
    } = overlay;

    SandboxSettings {
        enabled: Tighten::new(bool_true_is_stricter, format_display).merge(
            "sandbox.enabled",
            base_enabled,
            req_enabled,
            layer,
            location,
            sink,
        ),
        required: Tighten::new(bool_true_is_stricter, format_display).merge(
            "sandbox.required",
            base_required,
            req_required,
            layer,
            location,
            sink,
        ),
        allow_write: custom_allow_write(base_allow_write, req_allow_write, layer, location, sink),
        allow_network: Tighten::new(bool_false_is_stricter, format_display).merge(
            "sandbox.allow_network",
            base_allow_network,
            req_allow_network,
            layer,
            location,
            sink,
        ),
    }
}
