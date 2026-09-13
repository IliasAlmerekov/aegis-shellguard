use std::fmt::Display;

use crate::allowlist::ConfigSourceLayer;

use super::{SecurityRatchetWarning, push_ratchet_warning};

/// Ratchet an audit retention limit where a larger value keeps more history.
/// Global config remains last-wins. A project may raise the limit but cannot
/// lower the current value.
pub(crate) fn ratchet_audit_retention<T: Ord + Copy>(
    base: T,
    overlay: Option<T>,
    layer: ConfigSourceLayer,
) -> T {
    let requested = overlay.unwrap_or(base);
    match layer {
        ConfigSourceLayer::Global => requested,
        ConfigSourceLayer::Project => requested.max(base),
    }
}

pub(super) fn push_audit_retention_warning<T: Display + Ord + Copy>(
    warnings: &mut Vec<SecurityRatchetWarning>,
    field: &'static str,
    base: T,
    requested: Option<T>,
    location: &str,
) {
    let Some(requested) = requested else {
        return;
    };
    let kept = ratchet_audit_retention(base, Some(requested), ConfigSourceLayer::Project);
    push_ratchet_warning(
        warnings,
        field,
        requested.to_string(),
        kept.to_string(),
        location,
    );
}
