use std::path::Path;

use aegis::audit::Decision;
use aegis::config::amend::{
    AppendOutcome, active_config_path_for_append, append_allow_rule, append_block_rule,
};
use aegis::decision::BlockReason;
use aegis::interceptor::parser::{extract_prefix, split_tokens};
use aegis::planning::{CwdState, ExecutionDisposition, InterceptionPlan, PreparedPlanner};
use aegis::runtime::{AuditWriteOptions, RecoveryStatus, recovery_status};
use aegis::snapshot::SnapshotRecord;
use aegis::ui::confirm::{
    PromptDecision, RecoveryPromptDecision, show_confirmation, show_confirmation_decision,
    show_policy_block, show_recovery_override_decision,
};
use aegis_types::SandboxStatus;

use crate::shell_compat::{ShellLaunchOptions, exec_prepared_command, prepare_command};
use crate::{EXIT_BLOCKED, EXIT_DENIED, EXIT_INTERNAL};

fn persist_rule(
    cmd: &str,
    plan: &InterceptionPlan,
    append_fn: impl FnOnce(
        &std::path::Path,
        &[String],
        &std::path::Path,
    ) -> Result<AppendOutcome, aegis::config::ConfigError>,
    label: &str,
) -> Result<(), String> {
    match active_config_path_for_append() {
        Some(config_path) => {
            let tokens = split_tokens(cmd);
            let prefix = extract_prefix(&tokens);
            let cwd = match plan.decision_context().cwd_state() {
                CwdState::Resolved(path) => path.clone(),
                CwdState::Unavailable => std::path::PathBuf::from("."),
            };
            match append_fn(&config_path, &prefix, &cwd) {
                Ok(AppendOutcome::Conflict {
                    pattern,
                    existing_location,
                }) => {
                    let location = match existing_location {
                        aegis::config::allowlist::ConfigSourceLayer::Project => "project",
                        aegis::config::allowlist::ConfigSourceLayer::Global => "global",
                    };
                    eprintln!(
                        "warning: conflicting rule for '{pattern}' already exists in {location} config"
                    );
                }
                Ok(AppendOutcome::SkippedDuplicate | AppendOutcome::Appended) => {}
                Err(err) => return Err(format!("{err}")),
            }
        }
        None => {
            eprintln!("warning: cannot persist {label} rule: no config file found");
        }
    }
    Ok(())
}

pub(crate) fn run_planned_shell_command(
    cmd: &str,
    verbose: bool,
    prepared: &PreparedPlanner,
    plan: &InterceptionPlan,
    launch: &ShellLaunchOptions,
) -> i32 {
    let sandbox_config = match prepared {
        PreparedPlanner::Ready(context) => context.config().sandbox.as_ref(),
        PreparedPlanner::SetupFailure(_) => None,
    };

    match plan.execution_disposition() {
        ExecutionDisposition::Execute => execute_with_snapshots(
            cmd,
            verbose,
            prepared,
            plan,
            launch,
            Decision::AutoApproved,
            sandbox_config,
        ),
        ExecutionDisposition::RequiresApproval => {
            let prompt_decision =
                show_confirmation_decision(plan.assessment(), plan.explanation(), &[]);
            if prompt_decision == PromptDecision::ApproveAlways
                && let Err(err) = persist_rule(cmd, plan, append_allow_rule, "allow")
            {
                eprintln!("error: failed to append allow rule: {err}");
            }
            if prompt_decision == PromptDecision::DenyAlways
                && let Err(err) = persist_rule(cmd, plan, append_block_rule, "block")
            {
                eprintln!("error: failed to append block rule: {err}");
            }
            let approved = matches!(
                prompt_decision,
                PromptDecision::Approve | PromptDecision::ApproveAlways
            );
            if approved {
                execute_with_snapshots(
                    cmd,
                    verbose,
                    prepared,
                    plan,
                    launch,
                    Decision::Approved,
                    sandbox_config,
                )
            } else {
                if let Err(err) = append_shell_audit(
                    prepared,
                    plan,
                    Decision::Denied,
                    &[],
                    sandbox_status_before_preparation(sandbox_config),
                ) {
                    eprintln!("error: failed to write audit log: {err}");
                    return EXIT_INTERNAL;
                }
                EXIT_DENIED
            }
        }
        ExecutionDisposition::Block => {
            show_block_for_plan(plan);
            if let Err(err) = append_shell_audit(
                prepared,
                plan,
                Decision::Blocked,
                &[],
                sandbox_status_before_preparation(sandbox_config),
            ) {
                eprintln!("error: failed to write audit log: {err}");
                return EXIT_INTERNAL;
            }
            EXIT_BLOCKED
        }
    }
}

/// Create snapshots, append the audit entry, and execute the command.
///
/// This helper captures the shared ordering for auto-approved and
/// human-approved execution branches: snapshot creation happens only after the
/// final `Allow`/`AutoApproved` decision and before both the audit append and
/// the (optionally sandboxed) child process start. `Block` and `Denied`
/// branches never reach this function, so they cannot create snapshots.
fn execute_with_snapshots(
    cmd: &str,
    verbose: bool,
    prepared: &PreparedPlanner,
    plan: &InterceptionPlan,
    launch: &ShellLaunchOptions,
    decision: Decision,
    sandbox_config: Option<&aegis_sandbox::SandboxConfig>,
) -> i32 {
    let snapshots = create_snapshots_for_plan(prepared, plan, verbose);
    if let Some(RecoveryStatus::Degraded(degradation)) = recovery_status(
        plan.assessment().effect_opaque,
        plan.policy_decision().snapshots_required,
        &snapshots,
    ) {
        return match show_recovery_override_decision() {
            RecoveryPromptDecision::RunOnceWithoutRecovery => complete_approved_shell_execution(
                ShellExecution {
                    cmd,
                    launch,
                    sandbox_config,
                    prepared,
                    plan,
                    snapshots: &snapshots,
                    recovery_degradation: Some(degradation),
                },
                Decision::Approved,
            ),
            RecoveryPromptDecision::Deny => {
                if let Err(err) = append_shell_recovery_audit(
                    prepared,
                    plan,
                    Decision::Denied,
                    &snapshots,
                    sandbox_status_before_preparation(sandbox_config),
                    degradation,
                ) {
                    eprintln!("error: failed to write audit log: {err}");
                    return EXIT_INTERNAL;
                }
                EXIT_DENIED
            }
        };
    }

    complete_approved_shell_execution(
        ShellExecution {
            cmd,
            launch,
            sandbox_config,
            prepared,
            plan,
            snapshots: &snapshots,
            recovery_degradation: None,
        },
        decision,
    )
}

struct ShellExecution<'a> {
    cmd: &'a str,
    launch: &'a ShellLaunchOptions,
    sandbox_config: Option<&'a aegis_sandbox::SandboxConfig>,
    prepared: &'a PreparedPlanner,
    plan: &'a InterceptionPlan,
    snapshots: &'a [SnapshotRecord],
    recovery_degradation: Option<aegis_types::RecoveryDegradation>,
}

fn complete_approved_shell_execution(execution: ShellExecution<'_>, decision: Decision) -> i32 {
    let ShellExecution {
        cmd,
        launch,
        sandbox_config,
        prepared,
        plan,
        snapshots,
        recovery_degradation,
    } = execution;

    complete_shell_execution(
        decision,
        || prepare_command(cmd, launch, sandbox_config),
        |final_decision, sandbox_status| match recovery_degradation {
            Some(degradation) => append_shell_recovery_audit(
                prepared,
                plan,
                final_decision,
                snapshots,
                sandbox_status,
                degradation,
            ),
            None => append_shell_audit(prepared, plan, final_decision, snapshots, sandbox_status),
        },
        |message| eprintln!("warning: {message}"),
        exec_prepared_command,
        crate::shell_compat::wait_for_child,
        |message| eprintln!("error: {message}"),
    )
}

fn append_shell_recovery_audit(
    prepared: &PreparedPlanner,
    plan: &InterceptionPlan,
    decision: Decision,
    snapshots: &[SnapshotRecord],
    sandbox_status: SandboxStatus,
    degradation: aegis_types::RecoveryDegradation,
) -> Result<(), aegis::error::AegisError> {
    if let PreparedPlanner::Ready(context) = prepared {
        return context.append_audit_entry_with_recovery_degradation(
            plan.assessment(),
            decision,
            snapshots,
            plan.explanation(),
            AuditWriteOptions {
                allowlist_match: plan.decision_context().allowlist_match(),
                allowlist_effective: plan.policy_decision().allowlist_effective,
                ci_detected: plan.decision_context().ci_detected(),
                sandbox_status,
            },
            degradation,
        );
    }
    Ok(())
}

fn create_snapshots_for_plan(
    prepared: &PreparedPlanner,
    plan: &InterceptionPlan,
    verbose: bool,
) -> Vec<SnapshotRecord> {
    if matches!(
        plan.snapshot_plan(),
        aegis::planning::SnapshotPlan::NotRequired
    ) {
        return Vec::new();
    }

    match prepared {
        PreparedPlanner::Ready(context) => match plan.decision_context().cwd_state() {
            CwdState::Resolved(path) => {
                context.create_snapshots(path.as_path(), &plan.assessment().command.raw, verbose)
            }
            CwdState::Unavailable => {
                context.create_snapshots(Path::new("."), &plan.assessment().command.raw, verbose)
            }
        },
        PreparedPlanner::SetupFailure(_) => Vec::new(),
    }
}

fn append_shell_audit(
    prepared: &PreparedPlanner,
    plan: &InterceptionPlan,
    decision: Decision,
    snapshots: &[SnapshotRecord],
    sandbox_status: SandboxStatus,
) -> Result<(), aegis::error::AegisError> {
    if let PreparedPlanner::Ready(context) = prepared {
        return context.append_audit_entry(
            plan.assessment(),
            decision,
            snapshots,
            plan.explanation(),
            AuditWriteOptions {
                allowlist_match: plan.decision_context().allowlist_match(),
                allowlist_effective: plan.policy_decision().allowlist_effective,
                ci_detected: plan.decision_context().ci_detected(),
                sandbox_status,
            },
        );
    }
    Ok(())
}

/// Report an enabled Sandbox as `NotAttempted` before command preparation.
fn sandbox_status_before_preparation(
    sandbox_config: Option<&aegis_sandbox::SandboxConfig>,
) -> SandboxStatus {
    if sandbox_config.is_some() {
        SandboxStatus::NotAttempted
    } else {
        SandboxStatus::NotConfigured
    }
}

fn complete_shell_execution<C, X, AuditError>(
    decision: Decision,
    prepare: impl FnOnce() -> Result<(C, SandboxStatus), aegis_sandbox::SandboxError>,
    append_audit: impl FnOnce(Decision, SandboxStatus) -> Result<(), AuditError>,
    warn: impl FnOnce(&str),
    execute: impl FnOnce(C) -> Result<(X, SandboxStatus), aegis_sandbox::SandboxError>,
    wait: impl FnOnce(X) -> i32,
    report_error: impl FnOnce(&str),
) -> i32
where
    AuditError: std::fmt::Display,
{
    match prepare() {
        Ok((command, prep_status)) => {
            // Only Linux's inner Landlock wrapper can change `Active` after
            // preparation. It is held at its release gate, so reporting its
            // status before Audit does not start the wrapped program. Every
            // other launch path has a final preparation status and retains the
            // M1 audit-before-spawn ordering.
            let needs_inner_report =
                cfg!(target_os = "linux") && prep_status == SandboxStatus::Active;
            if !needs_inner_report {
                if let Err(err) = append_audit(decision, prep_status) {
                    report_error(&format!("failed to write audit log: {err}"));
                    return EXIT_INTERNAL;
                }
                if prep_status == SandboxStatus::Unavailable {
                    warn(aegis::runtime::SANDBOX_UNAVAILABLE_MESSAGE);
                }
                return match execute(command) {
                    Ok((child, _)) => wait(child),
                    Err(err) => {
                        report_error(&format!(
                            "Sandbox setup failed; command not executed: {err}"
                        ));
                        EXIT_INTERNAL
                    }
                };
            }

            match execute(command) {
                Ok((child, actual_status)) => {
                    if actual_status == SandboxStatus::NotAttempted {
                        if let Err(err) = append_audit(Decision::Blocked, actual_status) {
                            report_error(&format!("failed to write audit log: {err}"));
                            return EXIT_INTERNAL;
                        }
                        let _ = wait(child);
                        report_error(
                            "Sandbox setup failed; command not executed: inner sandbox wrapper did not report status",
                        );
                        return EXIT_INTERNAL;
                    }
                    if let Err(err) = append_audit(decision, actual_status) {
                        report_error(&format!("failed to write audit log: {err}"));
                        return EXIT_INTERNAL;
                    }
                    if actual_status == SandboxStatus::Unavailable {
                        warn(aegis::runtime::SANDBOX_UNAVAILABLE_MESSAGE);
                    }
                    wait(child)
                }
                Err(err) => {
                    if let Err(audit_err) =
                        append_audit(Decision::Blocked, SandboxStatus::NotAttempted)
                    {
                        report_error(&format!("failed to write audit log: {audit_err}"));
                        return EXIT_INTERNAL;
                    }
                    report_error(&format!(
                        "Sandbox setup failed; command not executed: {err}"
                    ));
                    EXIT_INTERNAL
                }
            }
        }
        Err(aegis_sandbox::SandboxError::Required) => {
            if let Err(err) = append_audit(Decision::Blocked, SandboxStatus::Unavailable) {
                report_error(&format!("failed to write audit log: {err}"));
                return EXIT_INTERNAL;
            }
            report_error(aegis::runtime::SANDBOX_REQUIRED_UNAVAILABLE_MESSAGE);
            EXIT_BLOCKED
        }
        Err(err) => {
            if let Err(audit_err) = append_audit(Decision::Blocked, SandboxStatus::NotAttempted) {
                report_error(&format!("failed to write audit log: {audit_err}"));
                return EXIT_INTERNAL;
            }
            report_error(&format!(
                "Sandbox setup failed; command not executed: {err}"
            ));
            EXIT_INTERNAL
        }
    }
}

fn show_block_for_plan(plan: &InterceptionPlan) {
    match plan.policy_decision().block_reason() {
        Some(BlockReason::ProtectCiPolicy) => {
            show_policy_block(plan.assessment(), plan.explanation())
        }
        Some(BlockReason::IntrinsicRiskBlock) => {
            show_confirmation(plan.assessment(), plan.explanation(), &[]);
        }
        Some(BlockReason::StrictPolicy) => {
            show_policy_block(plan.assessment(), plan.explanation());
        }
        Some(BlockReason::BlocklistOverride) => {
            show_policy_block(plan.assessment(), plan.explanation());
        }
        Some(BlockReason::PolicyRulesOverride) => {
            show_policy_block(plan.assessment(), plan.explanation());
        }
        None => {}
    }
}

#[cfg(test)]
mod sandbox_lifecycle_tests;
#[cfg(test)]
mod snapshot_ordering_tests;
#[cfg(test)]
mod test_support;

#[cfg(test)]
pub(crate) use test_support::decide_command;
