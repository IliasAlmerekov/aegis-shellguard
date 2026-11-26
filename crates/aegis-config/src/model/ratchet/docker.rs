use crate::allowlist::ConfigSourceLayer;
use crate::snapshot::DockerScopeMode;

use super::super::DockerScope;
use super::warning::{RatchetSink, is_project};

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

fn patterns_superset(overlay: &[String], base: &[String]) -> bool {
    base.iter().all(|p| overlay.contains(p))
}

fn docker_scope_narrows(base: &DockerScope, overlay: &DockerScope) -> bool {
    use DockerScopeMode::*;
    match (base.mode, overlay.mode) {
        (All, All) => false,
        (All, _) => true,
        (Labeled, All) => false,
        (Labeled, Labeled) => base.label != overlay.label,
        (Labeled, Names) => true,
        (Names, All) => false,
        (Names, Labeled) => true,
        (Names, Names) => !patterns_superset(&overlay.name_patterns, &base.name_patterns),
    }
}

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
