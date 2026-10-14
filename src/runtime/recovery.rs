//! Shared post-attempt Required recovery status.

use aegis_snapshot::SnapshotCoverage;
use aegis_types::{Mode, RecoveryDegradation};

use crate::planning::InterceptionPlan;

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
    use aegis_types::SnapshotRecord;

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
