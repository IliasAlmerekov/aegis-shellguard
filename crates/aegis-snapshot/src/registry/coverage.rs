use aegis_types::{SnapshotAttempt, SnapshotAttemptReadiness, SnapshotRecord};

use crate::readiness::ArtifactReadiness;

#[derive(Debug, Default, Clone)]
pub struct SnapshotCoverage {
    pub records: Vec<SnapshotRecord>,
    pub applicable: usize,
    pub unrecorded: Vec<&'static str>,
    pub attempts: Vec<SnapshotAttempt>,
}

pub(super) const COMMAND_READINESS_BUDGET: std::time::Duration =
    std::time::Duration::from_millis(500);

impl SnapshotCoverage {
    #[must_use]
    pub fn complete(records: Vec<SnapshotRecord>) -> Self {
        Self {
            applicable: records.len(),
            records,
            ..Self::default()
        }
    }

    pub fn merge(&mut self, other: SnapshotCoverage) {
        self.records.extend(other.records);
        self.applicable += other.applicable;
        self.unrecorded.extend(other.unrecorded);
        self.attempts.extend(other.attempts);
    }

    #[must_use]
    pub fn snapshot_attempts(&self) -> Vec<SnapshotAttempt> {
        let unchecked = self
            .records
            .iter()
            .skip(self.attempts.len())
            .map(|record| record.plugin);
        let failed = unchecked
            .chain(self.unrecorded.iter().copied())
            .map(|plugin| attempt(plugin, ArtifactReadiness::UNAVAILABLE));
        self.attempts.iter().cloned().chain(failed).collect()
    }

    #[must_use]
    pub fn has_unchecked_record(&self) -> bool {
        self.records.len() > self.attempts.len()
    }

    #[must_use]
    pub fn has_attempt(&self, readiness: SnapshotAttemptReadiness) -> bool {
        self.attempts.iter().any(|a| a.readiness == readiness)
    }

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
