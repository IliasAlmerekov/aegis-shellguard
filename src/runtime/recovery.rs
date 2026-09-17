//! Shared post-attempt Required recovery status.

use crate::snapshot::SnapshotCoverage;
use aegis_types::RecoveryDegradation;

/// Post-attempt state for an active ADR-016 Required recovery obligation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStatus {
    /// Every applicable Snapshot plugin created its required Snapshot.
    Ready,
    /// No required Snapshot was created, or only some applicable plugins
    /// created one.
    Degraded(RecoveryDegradation),
}

/// Derive the shared ADR-016 Recovery status from policy and runtime facts.
///
/// `coverage` must come from the snapshot pass that ran for this command, so
/// that the record count and the applicable-plugin count describe the same
/// moment (ADR-036).
#[must_use]
pub fn recovery_status(
    effect_opaque: bool,
    snapshots_required: bool,
    coverage: &SnapshotCoverage,
) -> Option<RecoveryStatus> {
    if !effect_opaque || !snapshots_required {
        return None;
    }

    Some(if coverage.records.is_empty() {
        RecoveryStatus::Degraded(RecoveryDegradation::NoSnapshotAvailable)
    } else if coverage.is_partial() {
        RecoveryStatus::Degraded(RecoveryDegradation::PartialSnapshotCoverage)
    } else {
        RecoveryStatus::Ready
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::SnapshotRecord;

    fn record(plugin: &'static str) -> SnapshotRecord {
        SnapshotRecord {
            plugin,
            snapshot_id: format!("{plugin}-snapshot"),
        }
    }

    #[test]
    fn required_recovery_is_ready_when_a_snapshot_was_created() {
        let coverage = SnapshotCoverage::complete(vec![SnapshotRecord {
            plugin: "git",
            snapshot_id: "stash@{0}".to_string(),
        }]);

        assert_eq!(
            recovery_status(true, true, &coverage),
            Some(RecoveryStatus::Ready)
        );
    }

    #[test]
    fn required_recovery_is_degraded_when_no_snapshot_was_created() {
        assert_eq!(
            recovery_status(true, true, &SnapshotCoverage::default()),
            Some(RecoveryStatus::Degraded(
                RecoveryDegradation::NoSnapshotAvailable
            ))
        );
    }

    #[test]
    fn total_failure_of_two_applicable_plugins_reports_no_snapshot_available() {
        let coverage = SnapshotCoverage {
            records: Vec::new(),
            applicable: 2,
        };

        assert_eq!(
            recovery_status(true, true, &coverage),
            Some(RecoveryStatus::Degraded(
                RecoveryDegradation::NoSnapshotAvailable
            ))
        );
    }

    #[test]
    fn one_failure_beside_one_success_reports_partial_coverage() {
        let coverage = SnapshotCoverage {
            records: vec![record("git")],
            applicable: 2,
        };

        assert_eq!(
            recovery_status(true, true, &coverage),
            Some(RecoveryStatus::Degraded(
                RecoveryDegradation::PartialSnapshotCoverage
            ))
        );
    }

    #[test]
    fn every_applicable_plugin_succeeding_stays_ready() {
        let coverage = SnapshotCoverage {
            records: vec![record("git"), record("sqlite")],
            applicable: 2,
        };

        assert_eq!(
            recovery_status(true, true, &coverage),
            Some(RecoveryStatus::Ready)
        );
    }

    #[test]
    fn more_records_than_applicable_plugins_stays_ready() {
        // Defensive: a caller that assembles coverage by hand cannot turn an
        // undercount of applicable plugins into a spurious degradation.
        let coverage = SnapshotCoverage {
            records: vec![record("git"), record("sqlite")],
            applicable: 1,
        };

        assert_eq!(
            recovery_status(true, true, &coverage),
            Some(RecoveryStatus::Ready)
        );
    }

    #[test]
    fn recovery_opt_out_has_no_required_status() {
        assert_eq!(
            recovery_status(true, false, &SnapshotCoverage::default()),
            None
        );
    }

    #[test]
    fn ordinary_danger_snapshot_failure_has_no_h9_recovery_status() {
        assert_eq!(
            recovery_status(false, true, &SnapshotCoverage::default()),
            None
        );
    }
}
