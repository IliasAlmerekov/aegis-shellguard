use std::path::PathBuf;

use crate::allowlist::ConfigSourceLayer;
use aegis_types::CeilingLayer;

use crate::ceiling::{WriteRoot, contains, intersect, is_lexically_invalid};

use super::super::SandboxSettings;
use super::super::partial::PartialSandboxSettings;
use super::direction::{Ratchet, Tighten, bool_false_is_stricter, format_display};
use super::warning::{RatchetSink, is_project};

pub(crate) type AllowWriteMerge = (Option<Vec<WriteRoot>>, bool, Option<Vec<WriteRoot>>);

pub(crate) fn custom_allow_write(
    base: Option<Vec<WriteRoot>>,
    base_narrows_computed: bool,
    base_trusted: Option<Vec<WriteRoot>>,
    overlay: Option<Vec<PathBuf>>,
    layer: ConfigSourceLayer,
    location: &str,
    sink: &mut RatchetSink,
) -> AllowWriteMerge {
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

pub(crate) fn merge_sandbox(
    base: SandboxSettings,
    overlay: PartialSandboxSettings,
    layer: ConfigSourceLayer,
    location: &str,
    sink: &mut RatchetSink,
) -> SandboxSettings {
    let SandboxSettings {
        allow_write: base_allow_write,
        allow_write_narrows_computed: base_narrows_computed,
        allow_write_trusted: base_trusted,
        allow_network: base_allow_network,
    } = base;
    let PartialSandboxSettings {
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
