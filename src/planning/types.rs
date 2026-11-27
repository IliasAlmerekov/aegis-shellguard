use crate::explanation::CommandExplanation;
use crate::explanation::formatter::build_explanation_from_plan;
use crate::planning::cwd::CwdState;
use crate::planning::git_target::GitTargetPlan;
use crate::runtime::AuditWriteOptions;
use aegis_config::AllowlistMatch;
use aegis_policy::{ExecutionTransport, PolicyAction, PolicyDecision};
use aegis_types::{Assessment, Mode, SandboxStatus};

pub enum PlanningOutcome {
    Planned(InterceptionPlan),
    SetupFailure(SetupFailurePlan),
}

pub struct InterceptionPlan {
    assessment: Box<Assessment>,
    decision_context: Box<DecisionContext>,
    policy_decision: PolicyDecision,
    snapshot_plan: SnapshotPlan,
    execution_disposition: ExecutionDisposition,
    explanation: Box<CommandExplanation>,
}

impl InterceptionPlan {
    pub fn audit_options(&self, sandbox_status: SandboxStatus) -> AuditWriteOptions<'_> {
        AuditWriteOptions {
            allowlist_match: self.decision_context().allowlist_match(),
            allowlist_effective: self.policy_decision().allowlist_effective,
            ci_detected: self.decision_context().ci_detected(),
            sandbox_status,
            snapshot_attempts: &[],
            recovery_degradation: None,
            watch: None,
        }
    }

    pub(crate) fn from_policy(
        assessment: Assessment,
        decision_context: DecisionContext,
        policy_decision: PolicyDecision,
    ) -> Self {
        let unresolved_gap =
            decision_context.git_targets.has_unresolved() && decision_context.mode != Mode::Audit;
        let unavailable_gap = matches!(decision_context.cwd_state, CwdState::Unavailable)
            && decision_context.mode != Mode::Audit
            && !decision_context.recovery_opt_out
            && (assessment.risk == aegis_types::RiskLevel::Danger || assessment.effect_opaque);
        let snapshot_plan =
            if policy_decision.snapshots_required || unresolved_gap || unavailable_gap {
                SnapshotPlan::Required {
                    applicable_plugins: decision_context.applicable_snapshot_plugins.clone(),
                    git_targets: decision_context.git_targets.clone(),
                }
            } else {
                SnapshotPlan::NotRequired
            };
        let execution_disposition = match policy_decision.decision {
            PolicyAction::AutoApprove => ExecutionDisposition::Execute,
            PolicyAction::Prompt => ExecutionDisposition::RequiresApproval,
            PolicyAction::Block => ExecutionDisposition::Block,
        };
        let mut explanation = build_explanation_from_plan(
            &assessment,
            decision_context.mode,
            decision_context.transport,
            decision_context.ci_detected,
            decision_context.allowlist_match.as_ref(),
            &decision_context.applicable_snapshot_plugins,
            policy_decision,
        );
        explanation.context.git_targets = decision_context.git_targets.explain();
        explanation.context.recovery_opt_out = decision_context.recovery_opt_out;
        if assessment.risk == aegis_types::RiskLevel::Danger
            && policy_decision.decision == PolicyAction::Prompt
            && !matches!(
                policy_decision.rationale,
                aegis_policy::PolicyRationale::AnalysisConfirmationRequired
                    | aegis_policy::PolicyRationale::AnalysisOverrideRequired
            )
            && !decision_context.recovery_opt_out
            && decision_context.applicable_snapshot_plugins.is_empty()
        {
            explanation.context.no_rollback_disclosure =
                Some(aegis_explanation::NO_ROLLBACK_DISCLOSURE.to_string());
        }

        Self {
            assessment: Box::new(assessment),
            decision_context: Box::new(decision_context),
            policy_decision,
            snapshot_plan,
            execution_disposition,
            explanation: Box::new(explanation),
        }
    }

    pub fn assessment(&self) -> &Assessment {
        self.assessment.as_ref()
    }

    pub fn decision_context(&self) -> &DecisionContext {
        &self.decision_context
    }

    pub fn policy_decision(&self) -> PolicyDecision {
        self.policy_decision
    }

    pub fn snapshot_plan(&self) -> SnapshotPlan {
        self.snapshot_plan.clone()
    }

    pub fn execution_disposition(&self) -> ExecutionDisposition {
        self.execution_disposition
    }

    pub fn explanation(&self) -> &CommandExplanation {
        self.explanation.as_ref()
    }
}

#[derive(Debug, Clone)]
pub struct SetupFailurePlan {
    user_message: String,
    is_config_fault: bool,
}

impl SetupFailurePlan {
    pub(crate) fn new(user_message: String, is_config_fault: bool) -> Self {
        Self {
            user_message,
            is_config_fault,
        }
    }

    pub fn is_config_fault(&self) -> bool {
        self.is_config_fault
    }

    pub fn user_message(&self) -> &str {
        &self.user_message
    }
}

#[derive(Debug, Clone)]
pub struct DecisionContext {
    mode: Mode,
    transport: ExecutionTransport,
    ci_detected: bool,
    cwd_state: CwdState,
    allowlist_match: Option<AllowlistMatch>,
    applicable_snapshot_plugins: Vec<&'static str>,
    git_targets: GitTargetPlan,
    recovery_opt_out: bool,
}

impl DecisionContext {
    pub(crate) fn new(
        mode: Mode,
        transport: ExecutionTransport,
        ci_detected: bool,
        cwd_state: CwdState,
        allowlist_match: Option<AllowlistMatch>,
        applicable_snapshot_plugins: Vec<&'static str>,
    ) -> Self {
        Self {
            mode,
            transport,
            ci_detected,
            cwd_state,
            allowlist_match,
            applicable_snapshot_plugins,
            git_targets: GitTargetPlan::default(),
            recovery_opt_out: false,
        }
    }

    pub(crate) fn with_git_targets(mut self, git_targets: GitTargetPlan) -> Self {
        self.git_targets = git_targets;
        self
    }

    pub(crate) fn with_recovery_opt_out(mut self, recovery_opt_out: bool) -> Self {
        self.recovery_opt_out = recovery_opt_out;
        self
    }

    pub fn git_targets(&self) -> &GitTargetPlan {
        &self.git_targets
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn transport(&self) -> ExecutionTransport {
        self.transport
    }

    pub fn ci_detected(&self) -> bool {
        self.ci_detected
    }

    pub fn cwd_state(&self) -> &CwdState {
        &self.cwd_state
    }

    pub fn allowlist_match(&self) -> Option<&AllowlistMatch> {
        self.allowlist_match.as_ref()
    }

    pub fn applicable_snapshot_plugins(&self) -> &[&'static str] {
        self.applicable_snapshot_plugins.as_slice()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotPlan {
    NotRequired,
    Required {
        applicable_plugins: Vec<&'static str>,
        git_targets: GitTargetPlan,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionDisposition {
    Execute,
    RequiresApproval,
    Block,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explanation::formatter::{
        build_explanation_from_plan, from_plan_inputs_call_count_for_tests,
        reset_from_plan_inputs_call_count_for_tests,
    };
    use aegis_config::allowlist::ConfigSourceLayer;
    use aegis_policy::{PolicyAction, PolicyDecision, PolicyRationale};
    use std::path::PathBuf;
    #[test]
    fn decision_context_constructor_preserves_read_access_via_getters() {
        let cwd_state = CwdState::Resolved(PathBuf::from("."));
        let allowlist_match = AllowlistMatch {
            pattern: "echo *".to_string(),
            reason: "trusted local echo".to_string(),
            source_layer: ConfigSourceLayer::Project,
        };
        let applicable_snapshot_plugins = vec!["git"];
        let context = DecisionContext::new(
            Mode::Protect,
            ExecutionTransport::Shell,
            true,
            cwd_state.clone(),
            Some(allowlist_match.clone()),
            applicable_snapshot_plugins.clone(),
        );

        assert_eq!(context.mode(), Mode::Protect);
        assert_eq!(context.transport(), ExecutionTransport::Shell);
        assert!(context.ci_detected());
        assert_eq!(context.cwd_state(), &cwd_state);
        assert_eq!(context.allowlist_match(), Some(&allowlist_match));
        assert_eq!(
            context.applicable_snapshot_plugins(),
            applicable_snapshot_plugins.as_slice()
        );
    }

    #[test]
    fn danger_without_provider_records_no_rollback_disclosure() {
        let assessment = aegis_scanner::assess("rm -rf ./tmp").unwrap();
        let context = DecisionContext::new(
            Mode::Protect,
            ExecutionTransport::Shell,
            false,
            CwdState::Resolved(PathBuf::from(".")),
            None,
            vec![],
        );
        let decision = PolicyDecision {
            decision: PolicyAction::Prompt,
            rationale: PolicyRationale::RequiresConfirmation,
            requires_confirmation: true,
            snapshots_required: true,
            confinement_required: false,
            allowlist_effective: false,
        };
        let plan = InterceptionPlan::from_policy(assessment, context, decision);
        let json = serde_json::to_value(plan.explanation()).unwrap();
        assert_eq!(
            json["context"]["no_rollback_disclosure"],
            "No Snapshot provider applies: this command cannot be rolled back."
        );
    }

    #[test]
    fn danger_with_provider_or_recovery_opt_out_has_no_no_rollback_disclosure() {
        use crate::planning::git_target::{GitTarget, GitTargetPlan, GitTargetUnresolved};
        let decision = PolicyDecision {
            decision: PolicyAction::Prompt,
            rationale: PolicyRationale::RequiresConfirmation,
            requires_confirmation: true,
            snapshots_required: true,
            confinement_required: false,
            allowlist_effective: false,
        };
        for (plugins, opt_out, unresolved) in [
            (vec!["git"], false, false),
            (vec![], true, false),
            (vec!["docker"], false, true),
        ] {
            let mut context = DecisionContext::new(
                Mode::Protect,
                ExecutionTransport::Shell,
                false,
                CwdState::Resolved(PathBuf::from(".")),
                None,
                plugins,
            )
            .with_recovery_opt_out(opt_out);
            if unresolved {
                context = context.with_git_targets(GitTargetPlan {
                    targets: vec![GitTarget::Unresolved(GitTargetUnresolved::DynamicPath)],
                    ..GitTargetPlan::default()
                });
            }
            let assessment = aegis_scanner::assess("rm -rf ./tmp").unwrap();
            let plan = InterceptionPlan::from_policy(assessment, context, decision);
            assert!(plan.explanation().context.no_rollback_disclosure.is_none());
        }
    }

    #[test]
    fn from_policy_builds_command_explanation_once() {
        let assessment = aegis_scanner::assess("rm -rf ./tmp").unwrap();
        let decision_context = DecisionContext::new(
            Mode::Protect,
            ExecutionTransport::Shell,
            false,
            CwdState::Resolved(PathBuf::from(".")),
            None,
            vec!["git"],
        );
        let policy_decision = PolicyDecision {
            decision: PolicyAction::Prompt,
            rationale: PolicyRationale::RequiresConfirmation,
            requires_confirmation: true,
            snapshots_required: true,
            confinement_required: false,
            allowlist_effective: false,
        };
        let expected_explanation = build_explanation_from_plan(
            &assessment,
            decision_context.mode(),
            decision_context.transport(),
            decision_context.ci_detected(),
            decision_context.allowlist_match(),
            decision_context.applicable_snapshot_plugins(),
            policy_decision,
        );
        reset_from_plan_inputs_call_count_for_tests();

        let plan = InterceptionPlan::from_policy(assessment, decision_context, policy_decision);

        assert_eq!(plan.explanation(), &expected_explanation);
        assert_eq!(from_plan_inputs_call_count_for_tests(), 1);
    }

    #[test]
    fn planning_keeps_allowlist_provenance_in_context_section() {
        let assessment = aegis_scanner::assess("cargo test --lib").unwrap();
        let allowlist_match = AllowlistMatch {
            pattern: "cargo test *".to_string(),
            reason: "safe local verification".to_string(),
            source_layer: ConfigSourceLayer::Global,
        };
        let decision_context = DecisionContext::new(
            Mode::Strict,
            ExecutionTransport::Shell,
            true,
            CwdState::Resolved(PathBuf::from(".")),
            Some(allowlist_match.clone()),
            vec!["git", "docker"],
        );
        let policy_decision = PolicyDecision {
            decision: PolicyAction::AutoApprove,
            rationale: PolicyRationale::AllowlistOverride,
            requires_confirmation: false,
            snapshots_required: false,
            confinement_required: false,
            allowlist_effective: true,
        };

        let plan = InterceptionPlan::from_policy(assessment, decision_context, policy_decision);

        let explanation = plan.explanation();
        let allowlist_explanation = explanation
            .context
            .allowlist_match
            .as_ref()
            .expect("planning should preserve allowlist provenance");

        assert_eq!(allowlist_explanation.pattern, allowlist_match.pattern);
        assert_eq!(allowlist_explanation.reason, allowlist_match.reason);
        assert_eq!(
            allowlist_explanation.source_layer,
            allowlist_match.source_layer
        );
    }
}
