//! Field-group merge for `SandboxSettings`. `enabled`/`required` are
//! `Tighten` (current behaviour — #229 tracks folding the Sandbox's mandatory
//! posture in here directly). `allow_write` is `Custom` (a set that can only
//! narrow, not last-wins). `allow_network` is `Tighten` with `false` as the
//! stricter direction.

use std::path::PathBuf;

use crate::allowlist::ConfigSourceLayer;
use crate::ceiling::{CeilingLayer, WriteRoot, contains, intersect, is_lexically_invalid};

use super::super::SandboxSettings;
use super::super::partial::PartialSandboxSettings;
use super::direction::{
    Ratchet, Tighten, bool_false_is_stricter, bool_true_is_stricter, format_display,
};
use super::warning::{RatchetSink, is_project};

/// Ratchet `sandbox.allow_write` (a list of path trees — more or wider roots =
/// weaker).
///
/// - Global layer: last-wins (a present overlay replaces the base, including
///   an explicit `[]`).
/// - Project layer: the base is narrowed by component-wise path-tree
///   intersection (no filesystem access: a ceiling path need not exist yet),
///   so trusted `/workspace/src` with project `/workspace` stays
///   `/workspace/src`. Any requested root not inside a trusted root is a
///   widening attempt: it is clamped and raises `project_security_ratchet`.
///   A relative or `..` entry is not clamped: it is kept so the ceiling
///   computation omits it with reason `relative` / `parent_dir`.
/// - Project list over an absent trusted list: the trusted base is the
///   computed default, unknown at merge. The list is kept with
///   `narrows_computed = true` and intersected with the computed default when
///   the ceiling is resolved, so the project can still only narrow.
///
/// Returns the merged list, the `narrows_computed` flag, and the trusted list
/// a project list narrowed (kept for the canonical containment check).
pub(crate) fn custom_allow_write(
    base: Option<Vec<WriteRoot>>,
    base_narrows_computed: bool,
    base_trusted: Option<Vec<WriteRoot>>,
    overlay: Option<Vec<PathBuf>>,
    layer: ConfigSourceLayer,
    location: &str,
    sink: &mut RatchetSink,
) -> (Option<Vec<WriteRoot>>, bool, Option<Vec<WriteRoot>>) {
    sink.touch("sandbox.allow_write");
    let Some(requested) = overlay else {
        return (base, base_narrows_computed, base_trusted);
    };
    let ceiling_layer = match layer {
        ConfigSourceLayer::Global => CeilingLayer::Global,
        ConfigSourceLayer::Project => CeilingLayer::Project,
    };
    let requested_roots: Vec<WriteRoot> = requested
        .iter()
        .map(|path| WriteRoot::new(path.clone(), ceiling_layer, location))
        .collect();
    if !is_project(layer) {
        return (Some(requested_roots), false, None);
    }
    let Some(trusted) = base else {
        return (Some(requested_roots), true, None);
    };

    // Relative and `..` entries are not widening attempts: they are kept
    // as written so the ceiling computation omits them with their own reason.
    let (invalid, requested_roots): (Vec<_>, Vec<_>) = requested_roots
        .into_iter()
        .partition(|root| is_lexically_invalid(&root.path));
    let mut kept = intersect(&trusted, &requested_roots);
    let widens = requested_roots
        .iter()
        .any(|req| !trusted.iter().any(|root| contains(&root.path, &req.path)));
    if widens {
        let requested_paths: Vec<&PathBuf> = requested_roots.iter().map(|r| &r.path).collect();
        let kept_paths: Vec<&PathBuf> = kept.iter().map(|root| &root.path).collect();
        sink.warn(
            "sandbox.allow_write",
            format!("{requested_paths:?}"),
            format!("{kept_paths:?}"),
            location,
        );
    }
    kept.extend(invalid);
    (
        Some(kept),
        base_narrows_computed,
        base_trusted.or(Some(trusted)),
    )
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
        allow_write_narrows_computed: base_narrows_computed,
        allow_write_trusted: base_trusted,
        allow_network: base_allow_network,
    } = base;
    let PartialSandboxSettings {
        enabled: req_enabled,
        required: req_required,
        allow_write: req_allow_write,
        allow_network: req_allow_network,
    } = overlay;

    let (allow_write, allow_write_narrows_computed, allow_write_trusted) = custom_allow_write(
        base_allow_write,
        base_narrows_computed,
        base_trusted,
        req_allow_write,
        layer,
        location,
        sink,
    );

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
        allow_write,
        allow_write_narrows_computed,
        allow_write_trusted,
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
