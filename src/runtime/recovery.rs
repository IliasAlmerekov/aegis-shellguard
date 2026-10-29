//! Shared post-attempt Required recovery status.

use aegis_snapshot::SnapshotCoverage;
use aegis_types::{Mode, RecoveryDegradation, SnapshotAttemptReadiness};

use crate::planning::InterceptionPlan;

/// Post-attempt state for an active ADR-016 Required recovery obligation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStatus {
    /// Every applicable Snapshot plugin created its required Snapshot and
    /// every attempt's artifact passed its readiness check.
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

    // Fixed priority (ADR-031 §9). Only record attempts count for the artifact
    // reasons: a plugin that left no record is Partial or None. A record with
    // no readiness verdict fails closed as Unavailable.
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

/// The Recovery degradation, if any, that must stop or interrupt this run.
///
/// Two sources. An unresolved Git target on an unattended run means the
/// Recovery obligation cannot be met: no git Snapshot covers the working tree
/// the command changes, and other Snapshots that succeeded do not change that
/// (#471). Otherwise the post-attempt [`recovery_status`] decides. `unattended`
/// is true when no human decided: the run was auto-approved. A confirmed run
/// keeps the weaker attended promise and discloses the gap in the dialog and
/// the audit entry instead. `Mode::Audit` declines all enforcement.
#[must_use]
pub fn plan_recovery_degradation(
    plan: &InterceptionPlan,
    unattended: bool,
    coverage: &SnapshotCoverage,
) -> Option<RecoveryDegradation> {
    let context = plan.decision_context();
    if unattended && context.mode() != Mode::Audit && context.git_targets().has_unresolved() {
        return Some(RecoveryDegradation::UnresolvedGitTarget);
    }
    match recovery_status(
        plan.assessment().effect_opaque,
        plan.policy_decision().snapshots_required,
        coverage,
    ) {
        Some(RecoveryStatus::Degraded(degradation)) => Some(degradation),
        Some(RecoveryStatus::Ready) | None => None,
    }
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
        // Defensive: a caller that assembles coverage by hand cannot turn an
        // undercount of applicable plugins into a spurious degradation.
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

    /// Coverage where `records` all produced a record, `unrecorded` plugins
    /// failed, and each record carries the given readiness.
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
        // The failed plugin's attempt reads `Unavailable` in the audit array,
        // yet no recorded artifact is missing: that is Partial, not Unavailable.
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

    // #471: an unresolved Git target halts unattended runs only.

    fn plan_for(command: &str, mode: Mode) -> InterceptionPlan {
        use crate::planning::{CwdState, PlanningOutcome, PlanningRequest, plan_with_context};
        use crate::runtime::RuntimeContext;
        use aegis_config::AegisConfig;
        use aegis_types::SnapshotPolicy;

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut config = AegisConfig::default();
        config.mode = mode;
        config.snapshot_policy = SnapshotPolicy::Selective;
        config.auto_snapshot_git = true;
        config.auto_snapshot_docker = false;
        let context = RuntimeContext::new(config, rt.handle().clone()).unwrap();
        let cwd = std::env::temp_dir();
        let outcome = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    plan_with_context(
                        &context,
                        PlanningRequest {
                            command,
                            cwd_state: CwdState::Resolved(cwd),
                            transport: aegis_policy::ExecutionTransport::Shell,
                            ci_detected: true,
                        },
                    )
                })
                .join()
                .unwrap()
        });
        let PlanningOutcome::Planned(plan) = outcome else {
            panic!("expected a normal plan");
        };
        plan
    }

    const UNRESOLVED: &str = "git -C /aegis-471-no-such-dir filter-branch --index-filter true";

    #[test]
    fn unresolved_git_target_halts_an_unattended_run_even_with_other_snapshots() {
        let plan = plan_for(UNRESOLVED, Mode::Protect);
        let coverage = SnapshotCoverage::complete(vec![record("git")]);

        assert_eq!(
            plan_recovery_degradation(&plan, true, &coverage),
            Some(RecoveryDegradation::UnresolvedGitTarget)
        );
    }

    #[test]
    fn unresolved_git_target_does_not_halt_a_confirmed_run() {
        let plan = plan_for(UNRESOLVED, Mode::Protect);

        assert_eq!(
            plan_recovery_degradation(&plan, false, &SnapshotCoverage::default()),
            None
        );
    }

    #[test]
    fn unresolved_git_target_does_not_halt_in_audit_mode() {
        let plan = plan_for(UNRESOLVED, Mode::Audit);

        assert_eq!(
            plan_recovery_degradation(&plan, true, &SnapshotCoverage::default()),
            None
        );
    }

    #[test]
    fn resolved_git_target_does_not_halt_an_unattended_run() {
        let dir = tempfile::TempDir::new().unwrap();
        let init = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .status()
            .unwrap();
        assert!(init.success());
        let command = format!(
            "git -C {} filter-branch --index-filter true",
            dir.path().display()
        );
        let plan = plan_for(&command, Mode::Protect);

        assert_eq!(
            plan_recovery_degradation(&plan, true, &SnapshotCoverage::default()),
            None
        );
    }
}
