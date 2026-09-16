//! `Custom` direction for `[[rules]]`. A project-layer entry that attempts to
//! auto-approve (`decision = "Allow"`, or `when.then = "Allow"`) is dropped
//! rather than honored — the project layer may only tighten via
//! `Prompt`/`Block`. Global-layer `[[rules]]` are trusted and always
//! appended, Allow included.

use crate::allowlist::ConfigSourceLayer;

use super::super::PolicyRule;
use super::context::is_untrusted_allow;
use super::direction::append;
use super::warning::{RatchetSink, is_project};

/// Merge one layer's `[[rules]]` overlay into the trusted base, dropping and
/// warning about any project-layer `Allow` entry.
pub(crate) fn custom_rules(
    base: Vec<PolicyRule>,
    overlay: Vec<PolicyRule>,
    layer: ConfigSourceLayer,
    location: &str,
    sink: &mut RatchetSink,
) -> Vec<PolicyRule> {
    sink.touch("rules");
    if is_project(layer) {
        let (dropped, kept): (Vec<_>, Vec<_>) = overlay.into_iter().partition(is_untrusted_allow);
        for rule in &dropped {
            sink.warn(
                "rules",
                format!("Allow({:?})", rule.pattern),
                "dropped".to_string(),
                location,
            );
        }
        append(base, kept)
    } else {
        append(base, overlay)
    }
}
