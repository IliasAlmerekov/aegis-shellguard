//! `Custom` direction for `docker_scope`.

use crate::allowlist::ConfigSourceLayer;
use crate::snapshot::DockerScopeMode;

use super::super::DockerScope;
use super::warning::{RatchetSink, is_project};

/// Docker breadth rank (higher = broader): `All` = 2; `Labeled` = 1; `Names`
/// with non-empty `name_patterns` = 1; `Names` with empty `name_patterns` = 0
/// (no-op). Used only to detect a no-op base/overlay (rank 0) — structural
/// narrowing is decided by [`docker_scope_narrows`].
fn docker_breadth_rank(scope: &DockerScope) -> u8 {
    match scope.mode {
        DockerScopeMode::All => 2,
        DockerScopeMode::Labeled => 1,
        DockerScopeMode::Names => {
            if scope.name_patterns.is_empty() {
                0
            } else {
                1
            }
        }
    }
}

/// True iff every pattern in `base` is present (as a literal string) in
/// `overlay` — i.e. `overlay` is a literal-string superset of `base`.
fn patterns_superset(overlay: &[String], base: &[String]) -> bool {
    base.iter().all(|p| overlay.contains(p))
}

/// Whether `overlay` narrows or is incomparable with `base`'s eligible-container
/// set (so the project must not win). `base` is assumed non-no-op (caller guards
/// via [`docker_breadth_rank`]).
///
/// Semantics: only keep-or-broaden moves are permitted.
/// - `All` is the broadest mode; anything else narrows from `All`.
/// - `Labeled` ↔ `Labeled` with the SAME label is a keep (no narrowing);
///   a different label is incomparable.
/// - `Names` → `Names` is a broaden/keep iff every base pattern is present in
///   the overlay (overlay is a literal-string superset).
/// - Any cross-mode switch between `Labeled` and `Names` is incomparable.
fn docker_scope_narrows(base: &DockerScope, overlay: &DockerScope) -> bool {
    use DockerScopeMode::*;
    match (base.mode, overlay.mode) {
        (All, All) => false, // identical effective (label/patterns unused)
        (All, _) => true,    // narrowing from broadest
        (Labeled, All) => false,
        (Labeled, Labeled) => base.label != overlay.label, // different label = incomparable
        (Labeled, Names) => true,                          // incomparable mode switch
        (Names, All) => false,
        (Names, Labeled) => true, // incomparable mode switch
        (Names, Names) => !patterns_superset(&overlay.name_patterns, &base.name_patterns),
    }
}

/// Ratchet the Docker snapshot scope. Under the Project layer, when the docker
/// provider is ENABLED in the trusted base AND the base scope is not a no-op
/// (rank 0), a project overlay that NARROWS or is INCOMPARABLE with the base
/// eligible-container set is rejected (keep base + warn). Only keep-or-broaden
/// moves are permitted. Global stays last-wins.
///
/// The destructure of `base`/`overlay` below is an exhaustive tripwire — a
/// new leaf field on `DockerScope` breaks this until it is added here.
pub(crate) fn custom_docker_scope(
    base: DockerScope,
    overlay: Option<DockerScope>,
    layer: ConfigSourceLayer,
    location: &str,
    provider_enabled_in_base: bool,
    sink: &mut RatchetSink,
) -> DockerScope {
    sink.touch("docker_scope");
    let DockerScope {
        mode: base_mode,
        label: base_label,
        name_patterns: base_name_patterns,
    } = base;
    let base = DockerScope {
        mode: base_mode,
        label: base_label,
        name_patterns: base_name_patterns,
    };

    let kept = match layer {
        ConfigSourceLayer::Global => overlay.clone().unwrap_or_else(|| base.clone()),
        ConfigSourceLayer::Project => {
            if !provider_enabled_in_base || docker_breadth_rank(&base) == 0 {
                overlay.clone().unwrap_or_else(|| base.clone())
            } else {
                match &overlay {
                    None => base.clone(),
                    Some(requested) if docker_scope_narrows(&base, requested) => base.clone(),
                    Some(requested) => requested.clone(),
                }
            }
        }
    };

    if is_project(layer)
        && let Some(DockerScope {
            mode,
            label,
            name_patterns,
        }) = overlay
    {
        let requested = DockerScope {
            mode,
            label,
            name_patterns,
        };
        sink.warn(
            "docker_scope",
            format!("{requested:?}"),
            format!("{kept:?}"),
            location,
        );
    }

    kept
}
