//! The closed set of Ratchet directions (CONTEXT.md "Ratchet direction",
//! ADR-013): `Tighten`, `GlobalOnly`, `Unratcheted`, plus the free-standing
//! `append` helper for the three concatenated vector fields. `Custom` rules
//! (provider targets, `docker_scope`, `[[rules]]`, prune limits,
//! `sandbox.allow_write`) are plain functions in the sibling modules —
//! wrapping them in a trait object would not add anything a named function
//! doesn't already give a call site.
//!
//! Every direction implements [`Ratchet::merge`], which both computes the
//! merged value and records the field path in the [`RatchetSink`] passed in
//! — the single pass `model::merge_layer` and the schema-coverage test both
//! rely on.

use crate::allowlist::ConfigSourceLayer;

use super::warning::{RatchetSink, is_project};

/// One field's rule for how a Project-layer overlay may change a trusted
/// Global-layer base.
pub(crate) trait Ratchet<T> {
    #[allow(clippy::too_many_arguments)]
    fn merge(
        &self,
        path: &'static str,
        base: T,
        overlay: Option<T>,
        layer: ConfigSourceLayer,
        location: &str,
        sink: &mut RatchetSink,
    ) -> T;
}

/// Keep the stricter of `base` and the requested value. `stricter(base,
/// requested)` returns whichever the field treats as more restrictive; under
/// the Global layer the requested value always wins (Global is trusted).
/// `ceiling`, when set, clamps the requested value before comparison — used
/// by the language-analysis budgets, which cap at every layer including
/// Global (ADR-022 §6). The clamp applies to the value kept, never to what
/// the warning reports: a project sees the number it actually wrote in its
/// file, not the capped one.
pub(crate) struct Tighten<T> {
    stricter: fn(T, T) -> T,
    format: fn(&T) -> String,
    ceiling: Option<fn(T) -> T>,
}

impl<T> Tighten<T> {
    pub(crate) fn new(stricter: fn(T, T) -> T, format: fn(&T) -> String) -> Self {
        Self {
            stricter,
            format,
            ceiling: None,
        }
    }

    pub(crate) fn with_ceiling(
        stricter: fn(T, T) -> T,
        format: fn(&T) -> String,
        ceiling: fn(T) -> T,
    ) -> Self {
        Self {
            stricter,
            format,
            ceiling: Some(ceiling),
        }
    }
}

impl<T: Clone> Ratchet<T> for Tighten<T> {
    fn merge(
        &self,
        path: &'static str,
        base: T,
        overlay: Option<T>,
        layer: ConfigSourceLayer,
        location: &str,
        sink: &mut RatchetSink,
    ) -> T {
        sink.touch(path);
        let Some(requested) = overlay else {
            return base;
        };
        let clamped = match self.ceiling {
            Some(clamp) => clamp(requested.clone()),
            None => requested.clone(),
        };
        let kept = match layer {
            ConfigSourceLayer::Global => clamped,
            ConfigSourceLayer::Project => (self.stricter)(base.clone(), clamped),
        };
        if is_project(layer) {
            sink.warn(
                path,
                (self.format)(&requested),
                (self.format)(&kept),
                location,
            );
        }
        kept
    }
}

/// The project layer cannot set this field at all — only Global config may.
/// Used for `language_analysis.trusted_aliases` (ADR-022 §6: "trusted global
/// aliases only"): a project must never introduce its own trusted interpreter
/// alias.
pub(crate) struct GlobalOnly<T> {
    format: fn(&T) -> String,
}

impl<T> GlobalOnly<T> {
    pub(crate) fn new(format: fn(&T) -> String) -> Self {
        Self { format }
    }
}

impl<T: Clone> Ratchet<T> for GlobalOnly<T> {
    fn merge(
        &self,
        path: &'static str,
        base: T,
        overlay: Option<T>,
        layer: ConfigSourceLayer,
        location: &str,
        sink: &mut RatchetSink,
    ) -> T {
        sink.touch(path);
        match layer {
            ConfigSourceLayer::Global => overlay.unwrap_or(base),
            ConfigSourceLayer::Project => {
                if let Some(requested) = &overlay {
                    sink.warn(
                        path,
                        (self.format)(requested),
                        (self.format)(&base),
                        location,
                    );
                }
                base
            }
        }
    }
}

/// The last layer to set this field wins, by deliberate decision rather than
/// oversight — `config_version` (schema version, not a security posture) and
/// `audit.compress_rotated` (storage format, not audit coverage: rotation and
/// retention are the ratcheted knobs that control how much history survives).
pub(crate) struct Unratcheted<T> {
    /// Why this field sits outside the ratchet, for readers of the merge
    /// call site. Not read at runtime.
    #[allow(dead_code)]
    reason: &'static str,
    _value: std::marker::PhantomData<T>,
}

impl<T> Unratcheted<T> {
    pub(crate) fn new(reason: &'static str) -> Self {
        Self {
            reason,
            _value: std::marker::PhantomData,
        }
    }
}

impl<T> Ratchet<T> for Unratcheted<T> {
    fn merge(
        &self,
        path: &'static str,
        base: T,
        overlay: Option<T>,
        _layer: ConfigSourceLayer,
        _location: &str,
        sink: &mut RatchetSink,
    ) -> T {
        sink.touch(path);
        overlay.unwrap_or(base)
    }
}

/// Append direction: `custom_patterns`, `allow`, `block`. Trusted entries
/// come first, project entries after — there is nothing to ratchet because
/// concatenation cannot remove a trusted entry, only add scoped ones capped
/// elsewhere (`allowlist_override_level`, blocklist always wins).
pub(crate) fn append<T>(base: Vec<T>, overlay: Vec<T>) -> Vec<T> {
    let mut merged = base;
    merged.extend(overlay);
    merged
}

pub(crate) fn format_debug<T: std::fmt::Debug>(value: &T) -> String {
    format!("{value:?}")
}

pub(crate) fn format_display<T: std::fmt::Display>(value: &T) -> String {
    value.to_string()
}

/// `stricter` for booleans where `true` is the stricter value (`sandbox.enabled`,
/// `sandbox.required`, all `auto_snapshot_*`).
pub(crate) fn bool_true_is_stricter(base: bool, requested: bool) -> bool {
    base || requested
}

/// `stricter` for booleans where `true` is the weaker value (`sandbox.allow_network`,
/// `audit.rotation_enabled`, `prune.enabled`).
pub(crate) fn bool_false_is_stricter(base: bool, requested: bool) -> bool {
    base && requested
}

/// `stricter` for retention-style limits where a larger value keeps more
/// history (`audit.max_file_size_bytes`, `audit.retention_files`).
pub(crate) fn keep_larger<T: Ord>(base: T, requested: T) -> T {
    requested.max(base)
}

/// `stricter` for budget-style limits where a smaller value is more
/// restrictive (language-analysis budgets).
pub(crate) fn keep_smaller<T: Ord>(base: T, requested: T) -> T {
    requested.min(base)
}
