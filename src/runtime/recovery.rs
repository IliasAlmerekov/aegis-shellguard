use aegis_snapshot::SnapshotCoverage;
use aegis_types::{RecoveryDegradation, SnapshotAttemptReadiness};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStatus {
    Ready,
    Degraded(RecoveryDegradation),
}

#[must_use]
pub fn recovery_status(
    obligated: bool,
    snapshots_required: bool,
    coverage: &SnapshotCoverage,
) -> Option<RecoveryStatus> {
    if !obligated || !snapshots_required {
        return None;
    }

    Some(if coverage.records.is_empty() {
        RecoveryStatus::Degraded(RecoveryDegradation::NoSnapshotAvailable)
    } else if coverage.has_attempt(SnapshotAttemptReadiness::Invalid) {
        RecoveryStatus::Degraded(RecoveryDegradation::SnapshotArtifactInvalid)
    } else if coverage.has_attempt(SnapshotAttemptReadiness::Unavailable)
        || coverage.has_unchecked_record()
    {
        RecoveryStatus::Degraded(RecoveryDegradation::SnapshotArtifactUnavailable)
    } else if coverage.is_partial() {
        RecoveryStatus::Degraded(RecoveryDegradation::PartialSnapshotCoverage)
    } else {
        RecoveryStatus::Ready
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_types::{
        SnapshotAttempt, SnapshotAttemptReadiness, SnapshotRecord, StructuralCheck, ValidationLevel,
    };

    fn record(plugin: &'static str) -> SnapshotRecord {
        SnapshotRecord {
            plugin,
            snapshot_id: format!("{plugin}-snapshot"),
            git_target: None,
        }
    }

    #[test]
    fn required_recovery_is_ready_when_a_snapshot_was_created() {
        let coverage = checked(&[("git", Ready)], &[]);

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
            ..Default::default()
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
        let coverage = checked(&[("git", Ready)], &["docker"]);

        assert_eq!(
            recovery_status(true, true, &coverage),
            Some(RecoveryStatus::Degraded(
                RecoveryDegradation::PartialSnapshotCoverage
            ))
        );
    }

    #[test]
    fn every_applicable_plugin_succeeding_stays_ready() {
        let coverage = checked(&[("git", Ready), ("sqlite", Ready)], &[]);

        assert_eq!(
            recovery_status(true, true, &coverage),
            Some(RecoveryStatus::Ready)
        );
    }

    #[test]
    fn more_records_than_applicable_plugins_stays_ready() {
        let coverage = SnapshotCoverage {
            applicable: 1,
            ..checked(&[("git", Ready), ("sqlite", Ready)], &[])
        };

        assert_eq!(
            recovery_status(true, true, &coverage),
            Some(RecoveryStatus::Ready)
        );
    }

    fn attempt(plugin: &str, readiness: SnapshotAttemptReadiness) -> SnapshotAttempt {
        SnapshotAttempt {
            plugin: plugin.to_string(),
            readiness,
            validation_level: ValidationLevel::PresenceOnly,
            structural_check: StructuralCheck::NotRun,
        }
    }

    fn checked(
        verdicts: &[(&'static str, SnapshotAttemptReadiness)],
        unrecorded: &[&'static str],
    ) -> SnapshotCoverage {
        SnapshotCoverage {
            records: verdicts.iter().map(|(p, _)| record(p)).collect(),
            applicable: verdicts.len() + unrecorded.len(),
            unrecorded: unrecorded.to_vec(),
            attempts: verdicts.iter().map(|(p, r)| attempt(p, *r)).collect(),
        }
    }

    fn degradation(coverage: &SnapshotCoverage) -> Option<RecoveryDegradation> {
        match recovery_status(true, true, coverage) {
            Some(RecoveryStatus::Degraded(reason)) => Some(reason),
            _ => None,
        }
    }

    use SnapshotAttemptReadiness::{Invalid, Ready, Unavailable};

    #[test]
    fn a_ready_attempt_beside_an_invalid_one_is_degraded_with_artifact_invalid() {
        let coverage = checked(&[("git", Ready), ("postgres", Invalid)], &[]);

        assert_eq!(
            recovery_status(true, true, &coverage),
            Some(RecoveryStatus::Degraded(
                RecoveryDegradation::SnapshotArtifactInvalid
            ))
        );
    }

    #[test]
    fn all_attempts_ready_stays_ready() {
        let coverage = checked(&[("git", Ready), ("sqlite", Ready)], &[]);

        assert_eq!(
            recovery_status(true, true, &coverage),
            Some(RecoveryStatus::Ready)
        );
    }

    #[test]
    fn a_lone_unavailable_attempt_is_artifact_unavailable() {
        let coverage = checked(&[("git", Unavailable)], &[]);

        assert_eq!(
            degradation(&coverage),
            Some(RecoveryDegradation::SnapshotArtifactUnavailable)
        );
    }

    #[test]
    fn priority_invalid_outranks_unavailable() {
        let coverage = checked(&[("git", Unavailable), ("postgres", Invalid)], &["docker"]);

        assert_eq!(
            degradation(&coverage),
            Some(RecoveryDegradation::SnapshotArtifactInvalid)
        );
    }

    #[test]
    fn priority_unavailable_outranks_partial_coverage() {
        let coverage = checked(&[("git", Unavailable), ("sqlite", Ready)], &["docker"]);

        assert_eq!(
            degradation(&coverage),
            Some(RecoveryDegradation::SnapshotArtifactUnavailable)
        );
    }

    #[test]
    fn priority_partial_coverage_outranks_no_snapshot_available() {
        let partial = checked(&[("git", Ready)], &["docker"]);
        let none = checked(&[], &["git", "docker"]);

        assert_eq!(
            (degradation(&partial), degradation(&none)),
            (
                Some(RecoveryDegradation::PartialSnapshotCoverage),
                Some(RecoveryDegradation::NoSnapshotAvailable)
            )
        );
    }

    #[test]
    fn a_plugin_without_a_record_is_not_a_recorded_artifact_gone_missing() {
        let coverage = checked(&[("git", Ready)], &["docker"]);

        assert_eq!(
            (
                degradation(&coverage),
                coverage.snapshot_attempts().last().map(|a| a.readiness)
            ),
            (
                Some(RecoveryDegradation::PartialSnapshotCoverage),
                Some(Unavailable)
            )
        );
    }

    #[test]
    fn a_record_that_was_never_readiness_checked_is_not_ready() {
        let coverage = SnapshotCoverage::complete(vec![record("git")]);

        assert_eq!(
            degradation(&coverage),
            Some(RecoveryDegradation::SnapshotArtifactUnavailable)
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
