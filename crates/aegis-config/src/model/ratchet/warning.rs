//! The warning type project-layer weakening attempts are reported through,
//! and the sink every [`super::direction::Ratchet`] call writes into.
//!
//! `RatchetSink` gives the merge path and the schema-coverage test (see
//! `model::tests::ratchet_coverage`) a single recording point: every
//! [`Ratchet::merge`](super::direction::Ratchet::merge) call touches its
//! field path here regardless of whether it warns, so a test build can prove
//! every declared field actually ran through the ratchet mechanism.

use crate::allowlist::ConfigSourceLayer;

/// A project-local config value attempted to weaken a security-critical setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SecurityRatchetWarning {
    pub(crate) field: &'static str,
    pub(crate) requested: String,
    pub(crate) kept: String,
    pub(crate) location: String,
}

/// Recording sink threaded through one layer merge.
///
/// `warnings` accumulates only the fields where the project layer's request
/// was not honored. `touched` (test builds only) accumulates every field
/// path a `Ratchet` direction ran for, whether or not it warned — this is
/// the record the coverage test diffs against the config's JSON schema.
#[derive(Debug, Default)]
pub(crate) struct RatchetSink {
    warnings: Vec<SecurityRatchetWarning>,
    #[cfg(test)]
    pub(crate) touched: Vec<&'static str>,
}

impl RatchetSink {
    /// Record that `path` went through the ratchet mechanism this merge,
    /// independent of whether it produced a warning.
    pub(crate) fn touch(
        &mut self,
        #[cfg_attr(not(test), allow(unused_variables))] path: &'static str,
    ) {
        #[cfg(test)]
        self.touched.push(path);
    }

    /// Push a warning for `field` when `kept` differs from what the project
    /// `requested`. Only called for the Project layer — Global is trusted and
    /// never warns.
    pub(crate) fn warn(
        &mut self,
        field: &'static str,
        requested: String,
        kept: String,
        location: &str,
    ) {
        if requested != kept {
            self.warnings.push(SecurityRatchetWarning {
                field,
                requested,
                kept,
                location: location.to_string(),
            });
        }
    }

    pub(crate) fn into_warnings(self) -> Vec<SecurityRatchetWarning> {
        self.warnings
    }
}

/// Shared layer-branch guard: only the Project layer can weaken, so callers
/// use this to skip `warn` calls on the Global layer.
pub(crate) fn is_project(layer: ConfigSourceLayer) -> bool {
    layer == ConfigSourceLayer::Project
}
