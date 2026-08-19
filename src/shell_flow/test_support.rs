//! Test-only scaffolding for the shell flow.
//!
//! These helpers reproduce the policy pipeline that `run_planned_shell_command`
//! drives, at a seam tests can call directly. They are `#[cfg(test)]` only and
//! carry no production behavior; `tests/main_thin_entrypoint.rs` asserts they do
//! not live in `main.rs`.

use std::path::Path;

use aegis::audit::Decision;
use aegis::config::AllowlistMatch;
use aegis::decision::BlockReason;
use aegis::decision::{
    ExecutionTransport, PolicyAction, PolicyAllowlistResult, PolicyBlocklistResult, PolicyCiState,
    PolicyConfigFlags, PolicyDecision, PolicyExecutionContext, PolicyInput, evaluate_policy,
};
use aegis::explanation::{
    AllowlistExplanation, CommandExplanation, ExecutionContextExplanation, PolicyExplanation,
    ScanExplanation,
};
use aegis::planning::evaluate_policy_rules;
use aegis::runtime::RuntimeContext;
use aegis::snapshot::SnapshotRecord;
use aegis::ui::confirm::{
    PromptDecision, show_confirmation, show_confirmation_decision, show_policy_block,
};

pub(crate) fn decide_command(
    context: &RuntimeContext,
    assessment: &aegis::interceptor::scanner::Assessment,
    cwd: &Path,
    verbose: bool,
    allowlist_match: Option<&AllowlistMatch>,
    in_ci: bool,
) -> (Decision, Vec<SnapshotRecord>, bool) {
    let (policy_decision, applicable_snapshot_plugins) = evaluate_policy_decision(
        context,
        assessment,
        cwd,
        allowlist_match,
        in_ci,
        ExecutionTransport::Shell,
    );
    let explanation = test_command_explanation(
        context,
        assessment,
        policy_decision,
        allowlist_match,
        in_ci,
        ExecutionTransport::Shell,
        &applicable_snapshot_plugins,
    );
    execute_policy_decision(
        context,
        assessment,
        cwd,
        policy_decision,
        &explanation,
        verbose,
    )
}

pub(super) fn execute_policy_decision(
    context: &RuntimeContext,
    assessment: &aegis::interceptor::scanner::Assessment,
    cwd: &Path,
    policy_decision: PolicyDecision,
    explanation: &CommandExplanation,
    verbose: bool,
) -> (Decision, Vec<SnapshotRecord>, bool) {
    match policy_decision.decision {
        PolicyAction::AutoApprove => {
            let snapshots = if policy_decision.snapshots_required {
                context.create_snapshots(cwd, &assessment.command.raw, verbose)
            } else {
                Vec::new()
            };
            (
                Decision::AutoApproved,
                snapshots,
                policy_decision.allowlist_effective,
            )
        }
        PolicyAction::Prompt => {
            let prompt_decision = show_confirmation_decision(assessment, explanation, &[]);
            let approved = matches!(
                prompt_decision,
                PromptDecision::Approve | PromptDecision::ApproveAlways
            );
            let decision = if approved {
                Decision::Approved
            } else {
                Decision::Denied
            };
            let snapshots = if approved && policy_decision.snapshots_required {
                context.create_snapshots(cwd, &assessment.command.raw, verbose)
            } else {
                Vec::new()
            };

            (decision, snapshots, policy_decision.allowlist_effective)
        }
        PolicyAction::Block => {
            match policy_decision.block_reason() {
                Some(BlockReason::ProtectCiPolicy) => show_policy_block(assessment, explanation),
                Some(BlockReason::IntrinsicRiskBlock) => {
                    show_confirmation(assessment, explanation, &[]);
                }
                Some(BlockReason::StrictPolicy) => {
                    show_policy_block(assessment, explanation);
                }
                Some(BlockReason::BlocklistOverride) => {
                    show_policy_block(assessment, explanation);
                }
                Some(BlockReason::PolicyRulesOverride) => {
                    show_policy_block(assessment, explanation);
                }
                None => unreachable!("PolicyAction::Block always carries a BlockReason"),
            }

            (
                Decision::Blocked,
                Vec::new(),
                policy_decision.allowlist_effective,
            )
        }
    }
}

pub(super) fn test_command_explanation(
    context: &RuntimeContext,
    assessment: &aegis::interceptor::scanner::Assessment,
    policy_decision: PolicyDecision,
    allowlist_match: Option<&AllowlistMatch>,
    in_ci: bool,
    transport: ExecutionTransport,
    applicable_snapshot_plugins: &[&'static str],
) -> CommandExplanation {
    CommandExplanation {
        scan: ScanExplanation {
            highest_risk: assessment.risk,
            decision_source: assessment.decision_source(),
            basis: assessment.basis(),
            matched_patterns: assessment
                .matched
                .iter()
                .map(|matched| aegis::explanation::ExplainedPatternMatch {
                    id: matched.pattern.id.to_string(),
                    risk: matched.pattern.risk,
                    description: matched.pattern.description.to_string(),
                    matched_text: matched.matched_text.clone(),
                    justification: matched.pattern.justification.as_deref().map(str::to_owned),
                })
                .collect(),
        },
        policy: PolicyExplanation {
            action: policy_decision.decision,
            rationale: policy_decision.rationale,
            requires_confirmation: policy_decision.requires_confirmation,
            snapshots_required: policy_decision.snapshots_required,
            allowlist_effective: policy_decision.allowlist_effective,
            block_reason: policy_decision.block_reason(),
        },
        context: ExecutionContextExplanation {
            mode: context.config().mode,
            transport,
            ci_detected: in_ci,
            allowlist_match: allowlist_match.map(|rule| AllowlistExplanation {
                pattern: rule.pattern.clone(),
                reason: rule.reason.clone(),
                source_layer: rule.source_layer,
            }),
            applicable_snapshot_plugins: applicable_snapshot_plugins
                .iter()
                .map(|plugin| (*plugin).to_string())
                .collect(),
        },
        outcome: None,
    }
}

fn evaluate_policy_decision(
    context: &RuntimeContext,
    assessment: &aegis::interceptor::scanner::Assessment,
    cwd: &Path,
    allowlist_match: Option<&AllowlistMatch>,
    in_ci: bool,
    transport: ExecutionTransport,
) -> (PolicyDecision, Vec<&'static str>) {
    let applicable_snapshot_plugins = if assessment.risk == aegis::interceptor::RiskLevel::Danger
        && context.config().snapshot_policy != aegis::config::SnapshotPolicy::None
    {
        context.applicable_snapshot_plugins(cwd)
    } else {
        Vec::new()
    };
    let decision = evaluate_policy(PolicyInput {
        assessment,
        mode: context.config().mode,
        ci_state: PolicyCiState { detected: in_ci },
        allowlist: PolicyAllowlistResult {
            matched: allowlist_match.is_some(),
        },
        blocklist: PolicyBlocklistResult {
            matched: context.is_blocked_for_command(&assessment.command.raw, Some(cwd)),
        },
        config_flags: PolicyConfigFlags {
            ci_policy: context.config().ci_policy,
            allowlist_override_level: context.config().strict_allowlist_override,
            snapshot_policy: context.config().snapshot_policy,
        },
        execution_context: PolicyExecutionContext {
            transport,
            applicable_snapshot_plugins: applicable_snapshot_plugins.as_slice(),
        },
        rules: evaluate_policy_rules(context.policy_rules(), &assessment.command.raw),
    });

    (decision, applicable_snapshot_plugins)
}
