//! Field-group merge for `LanguageAnalysisConfig` (ADR-022 §6). Every budget,
//! including `script_file_limit_bytes`, is `Tighten` with a hard ceiling that
//! clamps at every layer including Global; `trusted_aliases` is `GlobalOnly`
//! ("trusted global aliases only" — a project must never introduce its own
//! trusted interpreter alias).
//!
//! This replaces the old string-keyed `budget_fields()` dispatch: each budget
//! is now its own typed `Ratchet` call, so a typo in a field name is a
//! compile error instead of a silently skipped check.

use crate::allowlist::ConfigSourceLayer;

use super::super::partial::PartialLanguageAnalysisConfig;
use super::super::rules::{
    LANGUAGE_ANALYSIS_INLINE_SOURCE_MAX_BYTES, LANGUAGE_ANALYSIS_MAX_AGGREGATE_BYTES,
    LANGUAGE_ANALYSIS_MAX_DEPTH, LANGUAGE_ANALYSIS_MAX_SCRIPT_FILES, LANGUAGE_ANALYSIS_MAX_TARGETS,
    LANGUAGE_ANALYSIS_TIMEOUT_MS,
};
use super::super::{
    LANGUAGE_ANALYSIS_SCRIPT_FILE_HARD_CEILING_BYTES, LanguageAnalysisConfig, TrustedAlias,
};
use super::direction::{GlobalOnly, Ratchet, Tighten, format_debug, format_display, keep_smaller};
use super::warning::RatchetSink;

/// Merge one layer's `[language_analysis]` overlay into the trusted base. The
/// destructure of both structs is an exhaustive tripwire — a new
/// `LanguageAnalysisConfig` field breaks this until it declares a direction.
pub(crate) fn merge_language_analysis(
    base: LanguageAnalysisConfig,
    overlay: PartialLanguageAnalysisConfig,
    layer: ConfigSourceLayer,
    location: &str,
    sink: &mut RatchetSink,
) -> LanguageAnalysisConfig {
    let LanguageAnalysisConfig {
        inline_source_limit_bytes: base_inline_source_limit_bytes,
        script_file_limit_bytes: base_script_file_limit_bytes,
        max_script_files: base_max_script_files,
        max_depth: base_max_depth,
        max_targets: base_max_targets,
        max_aggregate_bytes: base_max_aggregate_bytes,
        timeout_ms: base_timeout_ms,
        trusted_aliases: base_trusted_aliases,
    } = base;
    let PartialLanguageAnalysisConfig {
        inline_source_limit_bytes: req_inline_source_limit_bytes,
        script_file_limit_bytes: req_script_file_limit_bytes,
        max_script_files: req_max_script_files,
        max_depth: req_max_depth,
        max_targets: req_max_targets,
        max_aggregate_bytes: req_max_aggregate_bytes,
        timeout_ms: req_timeout_ms,
        trusted_aliases: req_trusted_aliases,
    } = overlay;

    LanguageAnalysisConfig {
        inline_source_limit_bytes: Tighten::with_ceiling(
            keep_smaller::<u64>,
            format_display,
            |v: u64| v.min(LANGUAGE_ANALYSIS_INLINE_SOURCE_MAX_BYTES),
        )
        .merge(
            "language_analysis.inline_source_limit_bytes",
            base_inline_source_limit_bytes,
            req_inline_source_limit_bytes,
            layer,
            location,
            sink,
        ),
        script_file_limit_bytes: Tighten::with_ceiling(
            keep_smaller::<u64>,
            format_display,
            |v: u64| v.min(LANGUAGE_ANALYSIS_SCRIPT_FILE_HARD_CEILING_BYTES),
        )
        .merge(
            "language_analysis.script_file_limit_bytes",
            base_script_file_limit_bytes,
            req_script_file_limit_bytes,
            layer,
            location,
            sink,
        ),
        max_script_files: Tighten::with_ceiling(keep_smaller::<u64>, format_display, |v: u64| {
            v.min(LANGUAGE_ANALYSIS_MAX_SCRIPT_FILES)
        })
        .merge(
            "language_analysis.max_script_files",
            base_max_script_files,
            req_max_script_files,
            layer,
            location,
            sink,
        ),
        max_depth: Tighten::with_ceiling(keep_smaller::<u64>, format_display, |v: u64| {
            v.min(LANGUAGE_ANALYSIS_MAX_DEPTH)
        })
        .merge(
            "language_analysis.max_depth",
            base_max_depth,
            req_max_depth,
            layer,
            location,
            sink,
        ),
        max_targets: Tighten::with_ceiling(keep_smaller::<u64>, format_display, |v: u64| {
            v.min(LANGUAGE_ANALYSIS_MAX_TARGETS)
        })
        .merge(
            "language_analysis.max_targets",
            base_max_targets,
            req_max_targets,
            layer,
            location,
            sink,
        ),
        max_aggregate_bytes: Tighten::with_ceiling(
            keep_smaller::<u64>,
            format_display,
            |v: u64| v.min(LANGUAGE_ANALYSIS_MAX_AGGREGATE_BYTES),
        )
        .merge(
            "language_analysis.max_aggregate_bytes",
            base_max_aggregate_bytes,
            req_max_aggregate_bytes,
            layer,
            location,
            sink,
        ),
        timeout_ms: Tighten::with_ceiling(keep_smaller::<u64>, format_display, |v: u64| {
            v.min(LANGUAGE_ANALYSIS_TIMEOUT_MS)
        })
        .merge(
            "language_analysis.timeout_ms",
            base_timeout_ms,
            req_timeout_ms,
            layer,
            location,
            sink,
        ),
        trusted_aliases: GlobalOnly::<Vec<TrustedAlias>>::new(format_debug).merge(
            "language_analysis.trusted_aliases",
            base_trusted_aliases,
            req_trusted_aliases,
            layer,
            location,
            sink,
        ),
    }
}
