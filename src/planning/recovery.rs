use aegis_snapshot::SnapshotCoverage;
use aegis_types::{Mode, RecoveryDegradation, RiskLevel};

use crate::planning::{CwdState, InterceptionPlan};
use crate::runtime::{RecoveryStatus, recovery_status};

#[must_use]
pub fn plan_recovery_degradation(
    plan: &InterceptionPlan,
    unattended: bool,
    coverage: &SnapshotCoverage,
) -> Option<RecoveryDegradation> {
    let context = plan.decision_context();
    if context.mode() != Mode::Audit
        && matches!(context.cwd_state(), CwdState::Unavailable)
        && !plan.explanation().context.recovery_opt_out
        && (plan.assessment().risk == RiskLevel::Danger || plan.assessment().effect_opaque)
    {
        return Some(RecoveryDegradation::UnavailableCwd);
    }
    if unattended && context.mode() != Mode::Audit && context.git_targets().has_unresolved() {
        return Some(RecoveryDegradation::UnresolvedGitTarget);
    }
    let assessment = plan.assessment();
    let obligated =
        assessment.effect_opaque || (unattended && assessment.risk == RiskLevel::Danger);
    match recovery_status(
        obligated,
        plan.policy_decision().snapshots_required,
        coverage,
    ) {
        Some(RecoveryStatus::Degraded(degradation)) => Some(degradation),
        Some(RecoveryStatus::Ready) | None => None,
    }
}

#[must_use]
pub fn allow_rule_persistence_degradation(
    plan: &InterceptionPlan,
    coverage: &SnapshotCoverage,
) -> Option<RecoveryDegradation> {
    if matches!(plan.decision_context().cwd_state(), CwdState::Unavailable) {
        return Some(RecoveryDegradation::UnavailableCwd);
    }
    let assessment = plan.assessment();
    if !(assessment.effect_opaque || assessment.risk == RiskLevel::Danger) {
        return None;
    }
    if plan.decision_context().git_targets().has_unresolved() {
        return Some(RecoveryDegradation::UnresolvedGitTarget);
    }
    match recovery_status(true, true, coverage) {
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
            git_target: None,
        }
    }

    fn plan_for(command: &str, mode: Mode) -> InterceptionPlan {
        use crate::planning::{PlanningOutcome, PlanningRequest, plan_with_context};
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

    #[test]
    fn unavailable_cwd_halts_unattended_danger() {
        let plan = plan_with_unavailable_cwd();
        assert_eq!(
            plan_recovery_degradation(&plan, true, &SnapshotCoverage::default()),
            Some(RecoveryDegradation::UnavailableCwd)
        );
    }

    fn plan_with_unavailable_cwd() -> InterceptionPlan {
        use crate::planning::{PlanningOutcome, PlanningRequest, plan_with_context};
        use crate::runtime::RuntimeContext;
        let rt = tokio::runtime::Runtime::new().unwrap();
        let context =
            RuntimeContext::new(aegis_config::AegisConfig::default(), rt.handle().clone()).unwrap();
        let outcome = plan_with_context(
            &context,
            PlanningRequest {
                command: "rm -rf target",
                cwd_state: CwdState::Unavailable,
                transport: aegis_policy::ExecutionTransport::Shell,
                ci_detected: true,
            },
        );
        let PlanningOutcome::Planned(plan) = outcome else {
            panic!("expected plan")
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
        crate::test_git::git_init(dir.path());
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
