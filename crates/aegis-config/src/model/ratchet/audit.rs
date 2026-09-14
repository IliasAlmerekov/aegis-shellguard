//! Field-group merge for `AuditConfig`. Rotation and both retention limits
//! are `Tighten` (#267); `integrity_mode` is `Tighten` by rank
//! (`most_restrictive_integrity_mode`); `compress_rotated` is `Unratcheted`
//! — a storage-format choice, not audit coverage.

use crate::allowlist::ConfigSourceLayer;

use super::super::partial::PartialAuditConfig;
use super::super::{AuditConfig, most_restrictive_integrity_mode};
use super::direction::{
    Ratchet, Tighten, Unratcheted, bool_false_is_stricter, format_debug, format_display,
    keep_larger,
};
use super::warning::RatchetSink;

/// Merge one layer's `[audit]` overlay into the trusted base. The destructure
/// of both structs is an exhaustive tripwire — a new `AuditConfig` field
/// breaks this until it declares a direction.
pub(crate) fn merge_audit(
    base: AuditConfig,
    overlay: PartialAuditConfig,
    layer: ConfigSourceLayer,
    location: &str,
    sink: &mut RatchetSink,
) -> AuditConfig {
    let AuditConfig {
        rotation_enabled: base_rotation_enabled,
        max_file_size_bytes: base_max_file_size_bytes,
        retention_files: base_retention_files,
        compress_rotated: base_compress_rotated,
        integrity_mode: base_integrity_mode,
    } = base;
    let PartialAuditConfig {
        rotation_enabled: req_rotation_enabled,
        max_file_size_bytes: req_max_file_size_bytes,
        retention_files: req_retention_files,
        compress_rotated: req_compress_rotated,
        integrity_mode: req_integrity_mode,
    } = overlay;

    AuditConfig {
        rotation_enabled: Tighten::new(bool_false_is_stricter, format_display).merge(
            "audit.rotation_enabled",
            base_rotation_enabled,
            req_rotation_enabled,
            layer,
            location,
            sink,
        ),
        max_file_size_bytes: Tighten::new(keep_larger::<u64>, format_display).merge(
            "audit.max_file_size_bytes",
            base_max_file_size_bytes,
            req_max_file_size_bytes,
            layer,
            location,
            sink,
        ),
        retention_files: Tighten::new(keep_larger::<usize>, format_display).merge(
            "audit.retention_files",
            base_retention_files,
            req_retention_files,
            layer,
            location,
            sink,
        ),
        compress_rotated: Unratcheted::new(
            "storage format for rotated audit files, not audit coverage — rotation and retention are the ratcheted knobs",
        )
        .merge(
            "audit.compress_rotated",
            base_compress_rotated,
            req_compress_rotated,
            layer,
            location,
            sink,
        ),
        integrity_mode: Tighten::new(most_restrictive_integrity_mode, format_debug).merge(
            "audit.integrity_mode",
            base_integrity_mode,
            req_integrity_mode,
            layer,
            location,
            sink,
        ),
    }
}
