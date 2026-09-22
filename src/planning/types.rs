//! Planning types: requests, outcomes, dispositions, and contexts.

use std::path::PathBuf;

use crate::explanation::CommandExplanation;
use crate::explanation::formatter::build_explanation_from_plan;
use aegis_config::AllowlistMatch;
use aegis_policy::{ExecutionTransport, PolicyAction, PolicyDecision};
use aegis_types::{Assessment, Mode};

/// Canonical planning result shared by interception surfaces.
pub enum PlanningOutcome {
    /// A normal command plan produced from scanner + policy inputs.
    Planned(InterceptionPlan),
    /// A fail-closed setup outcome produced before normal planning could finish.
    SetupFailure(SetupFailurePlan),
}

/// Canonical typed plan for one intercepted command.
pub struct InterceptionPlan {
    assessment: Box<Assessment>,
    decision_context: DecisionContext,
    policy_decision: PolicyDecision,
    snapshot_plan: SnapshotPlan,
    execution_disposition: ExecutionDisposition,
    explanation: Box<CommandExplanation>,
}

impl InterceptionPlan {
    /// Build a canonical interception plan from a pure policy result.
    pub(crate) fn from_policy(
        assessment: Assessment,
        decision_context: DecisionContext,
        policy_decision: PolicyDecision,
    ) -> Self {
        let snapshot_plan = if policy_decision.snapshots_required {
            SnapshotPlan::Required {
                applicable_plugins: decision_context.applicable_snapshot_plugins.clone(),
            }
        } else {
            SnapshotPlan::NotRequired
        };
        let execution_disposition = match policy_decision.decision {
            PolicyAction::AutoApprove => ExecutionDisposition::Execute,
            PolicyAction::Prompt => ExecutionDisposition::RequiresApproval,
            PolicyAction::Block => ExecutionDisposition::Block,
        };
        let explanation = build_explanation_from_plan(
            &assessment,
            decision_context.mode,
            decision_context.transport,
            decision_context.ci_detected,
            decision_context.allowlist_match.as_ref(),
            &decision_context.applicable_snapshot_plugins,
            policy_decision,
        );

        Self {
            assessment: Box::new(assessment),
            decision_context,
            policy_decision,
            snapshot_plan,
            execution_disposition,
            explanation: Box::new(explanation),
        }
    }

    /// Return the scanner assessment used to build the plan.
    pub fn assessment(&self) -> &Assessment {
        self.assessment.as_ref()
    }

    /// Return the resolved decision context used to evaluate policy.
    pub fn decision_context(&self) -> &DecisionContext {
        &self.decision_context
    }

    /// Return the pure policy decision embedded in the plan.
    pub fn policy_decision(&self) -> PolicyDecision {
        self.policy_decision
    }

    /// Return the pre-execution snapshot requirements for this plan.
    pub fn snapshot_plan(&self) -> SnapshotPlan {
        self.snapshot_plan.clone()
    }

    /// Return what the caller must do next with this command.
    pub fn execution_disposition(&self) -> ExecutionDisposition {
        self.execution_disposition
    }

    /// Return the descriptive explanation assembled during planning.
    pub fn explanation(&self) -> &CommandExplanation {
        self.explanation.as_ref()
    }
}

/// Typed fail-closed planning result for setup failures.
#[derive(Debug, Clone)]
pub struct SetupFailurePlan {
    user_message: String,
    is_config_fault: bool,
}

impl SetupFailurePlan {
    /// Create a fail-closed setup failure plan.
    pub(crate) fn new(user_message: String, is_config_fault: bool) -> Self {
        Self {
            user_message,
            is_config_fault,
        }
    }

    /// Whether the underlying failure stems from invalid user configuration.
    ///
    /// Mirrors [`crate::error::AegisError::is_config_fault`], computed once
    /// when the plan was built from the originating error.
    pub fn is_config_fault(&self) -> bool {
        self.is_config_fault
    }

    /// Return the user-facing setup failure message.
    pub fn user_message(&self) -> &str {
        &self.user_message
    }
}

/// Typed planning context resolved before pure policy evaluation.
#[derive(Debug, Clone)]
pub struct DecisionContext {
    mode: Mode,
    transport: ExecutionTransport,
    ci_detected: bool,
    cwd_state: CwdState,
    allowlist_match: Option<AllowlistMatch>,
    applicable_snapshot_plugins: Vec<&'static str>,
}

impl DecisionContext {
    /// Construct a decision context with all policy-relevant inputs resolved.
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
        }
    }

    /// Return the effective execution mode.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Return the caller transport requesting the decision.
    pub fn transport(&self) -> ExecutionTransport {
        self.transport
    }

    /// Return whether CI was detected for this invocation.
    pub fn ci_detected(&self) -> bool {
        self.ci_detected
    }

    /// Return the working-directory resolution state.
    pub fn cwd_state(&self) -> &CwdState {
        &self.cwd_state
    }

    /// Return the matching allowlist entry for the command in this context, if any.
    pub fn allowlist_match(&self) -> Option<&AllowlistMatch> {
        self.allowlist_match.as_ref()
    }

    /// Return the snapshot plugins applicable to the resolved cwd.
    pub fn applicable_snapshot_plugins(&self) -> &[&'static str] {
        self.applicable_snapshot_plugins.as_slice()
    }
}

/// Working-directory resolution state visible to planning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CwdState {
    /// The command cwd was resolved successfully.
    Resolved(PathBuf),
    /// The command cwd could not be resolved.
    Unavailable,
}

/// Snapshot requirement derived from policy and cwd context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotPlan {
    /// No snapshots are required before execution.
    NotRequired,
    /// Snapshots are required, with the applicable plugin set already resolved.
    Required {
        /// Names of snapshot plugins applicable to this command.
        applicable_plugins: Vec<&'static str>,
    },
}

/// Next-step execution handling required by the plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionDisposition {
    /// Execute immediately.
    Execute,
    /// Require approval before execution.
    RequiresApproval,
    /// Hard-block execution.
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
