use aegis::planning::{InterceptionPlan, PreparedPlanner};

use crate::shell_compat::ShellLaunchOptions;

pub(crate) fn run_planned_shell_command(
    cmd: &str,
    verbose: bool,
    prepared: &PreparedPlanner,
    plan: &InterceptionPlan,
    launch: &ShellLaunchOptions,
) -> i32 {
    crate::execution::run_planned_shell_command(cmd, verbose, prepared, plan, launch)
}

#[cfg(test)]
pub(crate) fn decide_command(
    context: &aegis::runtime::RuntimeContext,
    assessment: &aegis_types::Assessment,
    cwd: &std::path::Path,
    _verbose: bool,
    allowlist_match: Option<&aegis_config::AllowlistMatch>,
    in_ci: bool,
) -> (
    aegis_types::Decision,
    Vec<aegis_types::SnapshotRecord>,
    bool,
) {
    use aegis::planning::evaluate_policy_rules;
    use aegis_policy::{
        ExecutionTransport, PolicyAction, PolicyAllowlistResult, PolicyBlocklistResult,
        PolicyCiState, PolicyConfigFlags, PolicyExecutionContext, PolicyInput, evaluate_policy,
    };

    let policy_decision = evaluate_policy(PolicyInput {
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
            transport: ExecutionTransport::Shell,
            applicable_snapshot_plugins: &[],
        },
        rules: evaluate_policy_rules(context.policy_rules(), &assessment.command.raw),
    });
    let decision = match policy_decision.decision {
        PolicyAction::AutoApprove => aegis_types::Decision::AutoApproved,
        PolicyAction::Prompt => aegis_types::Decision::Denied,
        PolicyAction::Block => aegis_types::Decision::Blocked,
    };
    (decision, Vec::new(), policy_decision.allowlist_effective)
}
