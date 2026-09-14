//! Project-config security ratchet (ADR-013, CONTEXT.md "Ratchet direction").
//!
//! Every field of [`super::AegisConfig`] and its nested config structs
//! declares one of the five closed directions in `direction`: `Tighten`,
//! `GlobalOnly`, `Append`, `Unratcheted`, or a named `Custom` function.
//! `model::merge_layer` destructures `AegisConfig` and every nested struct
//! exhaustively and routes each field through its declared direction, so
//! adding a field without picking one is a compile error, not a silent
//! last-wins default. The one exception is the `#[serde(skip)]` bookkeeping
//! fields, which a project file cannot write: they carry no direction and
//! stay out of ADR-013's table, but they still pass through the destructure
//! behind the `Provenance` marker in `provenance`.
//!
//! Both the merge itself and the warnings a project layer's weakening
//! attempts produce come out of that ONE pass — there is no separate
//! re-parse-and-recompute warning collector. `project_security_ratchet_warnings`
//! below is a thin wrapper over the same merge, kept for the callers (and
//! tests) that only want the warnings.

#[cfg(test)]
use super::{AegisConfig, ConfigLayerPath};
#[cfg(test)]
use crate::allowlist::ConfigSourceLayer;

mod audit;
mod context;
mod direction;
mod docker;
mod language;
mod policy;
mod provenance;
mod prune;
mod sandbox;
mod supabase;
mod targets;
mod warning;

pub(crate) use context::RatchetContext;
pub(crate) use warning::SecurityRatchetWarning;

pub(super) use audit::merge_audit;
pub(super) use docker::custom_docker_scope;
pub(super) use language::merge_language_analysis;
pub(super) use policy::custom_rules;
pub(super) use prune::merge_prune;
pub(super) use sandbox::merge_sandbox;
pub(super) use supabase::custom_supabase_snapshot;
pub(super) use targets::{custom_mysql_snapshot, custom_postgres_snapshot, custom_sqlite_snapshot};
pub(super) use warning::RatchetSink;

// Only test call sites use this directly today (production code gets
// warnings from `merge_layer_path_with_warnings`'s single pass) — see
// `model::tests::ratchet_helpers::project_ratchet_warnings` and the C3 suites.
#[cfg(test)]
impl AegisConfig {
    /// Compare a project layer's requested values against the current base
    /// config and report any security-critical weakening attempts that the
    /// ratchet will ignore during merge. Thin wrapper over
    /// [`AegisConfig::merge_layer_path_with_warnings`] — same merge, warnings only.
    pub(crate) fn project_security_ratchet_warnings(
        base: &Self,
        layer: &ConfigLayerPath,
    ) -> std::result::Result<Vec<SecurityRatchetWarning>, crate::error::ConfigError> {
        if layer.source_layer != ConfigSourceLayer::Project {
            return Ok(Vec::new());
        }
        let (_, warnings) = Self::merge_layer_path_with_warnings(base.clone(), layer)?;
        Ok(warnings)
    }
}

// Re-exported so `model.rs`'s exhaustive `merge_layer` destructure can name
// these without a `super::ratchet::direction::` prefix on every call.
pub(super) use direction::{
    Ratchet, Tighten, Unratcheted, append, bool_true_is_stricter, format_debug, format_display,
};
pub(super) use provenance::Provenance;
