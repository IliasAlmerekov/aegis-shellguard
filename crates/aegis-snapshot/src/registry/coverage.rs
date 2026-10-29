//! What a Snapshot pass produced: records, attempts, and coverage.

use aegis_types::{SnapshotAttempt, SnapshotAttemptReadiness, SnapshotRecord};

use crate::readiness::ArtifactReadiness;

/// What one [`SnapshotRegistry::snapshot_all`] pass produced and how much of
/// the applicable provider set it covers.
///
/// `records` holds the snapshots that were created. `applicable` counts the
/// plugins that reported themselves applicable during the same pass, whether
/// or not their attempt succeeded. `records.len() < applicable` means at least
/// one applicable plugin failed and the coverage is partial. Both numbers come
/// from one pass on purpose: sampling applicability again after the attempts
/// can return a different set, and a recovery decision must not rest on two
/// disagreeing samples.
#[derive(Debug, Default, Clone)]
pub struct SnapshotCoverage {
    /// The snapshots that were created, one per successful plugin attempt.
    pub records: Vec<SnapshotRecord>,
    /// How many plugins reported themselves applicable during the pass.
    pub applicable: usize,
    /// Applicable plugins whose attempt failed before leaving a record, one
    /// entry per failed attempt.
    pub unrecorded: Vec<&'static str>,
    /// The readiness verdict of each record, in `records` order. Empty until
    /// [`SnapshotRegistry::check_readiness`] has run on this coverage.
    pub attempts: Vec<SnapshotAttempt>,
}

/// Wall-clock budget for all readiness checks of one command (ADR-031 §6).
pub(super) const COMMAND_READINESS_BUDGET: std::time::Duration =
    std::time::Duration::from_millis(500);

impl SnapshotCoverage {
    /// Build a coverage result from records alone, treating every record as a
    /// successful attempt by an applicable plugin.
    ///
    /// For callers that never ran a registry pass, such as a code path that
    /// short-circuits before snapshot creation.
    #[must_use]
    pub fn complete(records: Vec<SnapshotRecord>) -> Self {
        Self {
            applicable: records.len(),
            records,
            ..Self::default()
        }
    }

    /// Fold another pass into this one: records append and applicable
    /// counts add, so partial coverage stays visible across passes.
    pub fn merge(&mut self, other: SnapshotCoverage) {
        self.records.extend(other.records);
        self.applicable += other.applicable;
        self.unrecorded.extend(other.unrecorded);
        self.attempts.extend(other.attempts);
    }

    /// Every attempt this coverage made, one per attempt (ADR-031 §10): the
    /// checked records, then each failed attempt as `Unavailable` since it
    /// left no artifact to check.
    #[must_use]
    pub fn snapshot_attempts(&self) -> Vec<SnapshotAttempt> {
        let failed = self
            .unrecorded
            .iter()
            .map(|plugin| attempt(plugin, ArtifactReadiness::UNAVAILABLE));
        self.attempts.iter().cloned().chain(failed).collect()
    }

    /// Whether any checked attempt has this readiness.
    #[must_use]
    pub fn has_attempt(&self, readiness: SnapshotAttemptReadiness) -> bool {
        self.attempts.iter().any(|a| a.readiness == readiness)
    }

    /// Returns `true` when an applicable plugin failed while another succeeded.
    #[must_use]
    pub fn is_partial(&self) -> bool {
        !self.records.is_empty() && self.records.len() < self.applicable
    }
}

pub(super) fn attempt(plugin: &str, outcome: ArtifactReadiness) -> SnapshotAttempt {
    SnapshotAttempt {
        plugin: plugin.to_string(),
        readiness: outcome.readiness,
        validation_level: outcome.validation_level,
        structural_check: outcome.structural_check,
    }
}
