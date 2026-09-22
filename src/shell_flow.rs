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
    use aegis::planning::{CwdState, PlanningRequest};
    use aegis_tui::{ExecutionRenderer, PromptDecision, TestRenderer};

    let outcome = aegis::planning::plan_with_context(
        context,
        PlanningRequest {
            command: &assessment.command.raw,
            cwd_state: CwdState::Resolved(cwd.to_path_buf()),
            transport: aegis_policy::ExecutionTransport::Shell,
            ci_detected: in_ci,
        },
    );
    let aegis::planning::PlanningOutcome::Planned(plan) = outcome else {
        return (aegis_types::Decision::Blocked, Vec::new(), false);
    };
    if allowlist_match.is_some()
        && ((assessment.risk == aegis_types::RiskLevel::Warn
            && matches!(
                context.config().strict_allowlist_override,
                aegis_types::AllowlistOverrideLevel::Warn
                    | aegis_types::AllowlistOverrideLevel::Danger
            ))
            || (assessment.risk == aegis_types::RiskLevel::Danger
                && matches!(
                    context.config().strict_allowlist_override,
                    aegis_types::AllowlistOverrideLevel::Danger
                )))
    {
        return (aegis_types::Decision::AutoApproved, Vec::new(), true);
    }
    let renderer = TestRenderer::new();
    let decision = match plan.execution_disposition() {
        aegis::planning::ExecutionDisposition::Execute => aegis_types::Decision::AutoApproved,
        aegis::planning::ExecutionDisposition::RequiresApproval => {
            renderer.set_confirmation_decision(PromptDecision::Deny);
            match renderer.show_confirmation(plan.assessment(), plan.explanation(), &[]) {
                PromptDecision::Approve | PromptDecision::ApproveAlways => {
                    aegis_types::Decision::Approved
                }
                _ => aegis_types::Decision::Denied,
            }
        }
        aegis::planning::ExecutionDisposition::Block => aegis_types::Decision::Blocked,
    };
    (
        decision,
        Vec::new(),
        plan.policy_decision().allowlist_effective,
    )
}
