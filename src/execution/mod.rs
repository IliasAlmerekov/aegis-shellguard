use aegis::error::AegisError;
use aegis::planning::{InterceptionPlan, PreparedPlanner};
use aegis::runtime::disposition::{
    AuditRecord, DispositionOutput, RecoveryGate, SandboxFailure, append_audit, decide,
    recovery_gate, setup_failed_message, snapshot_request,
};
use aegis_snapshot::SnapshotCoverage;
use aegis_tui::{ExecutionRenderer, PromptDecision, RecoveryPromptDecision, TerminalRenderer};
use aegis_types::{Decision, RecoveryDegradation, SandboxStatus};

use crate::shell_compat::{ShellLaunchOptions, exec_prepared_command, prepare_command};
use crate::{EXIT_BLOCKED, EXIT_DENIED, EXIT_INTERNAL};

struct ShellOutput<'a, R>(&'a R);

impl<R: ExecutionRenderer> DispositionOutput for ShellOutput<'_, R> {
    fn confirm(&mut self, plan: &InterceptionPlan) -> PromptDecision {
        self.0
            .show_confirmation(plan.assessment(), plan.explanation(), &[])
    }

    fn recovery_override(&mut self, degradation: RecoveryDegradation) -> RecoveryPromptDecision {
        self.0.show_recovery_override(degradation)
    }

    fn show_policy_block(&mut self, plan: &InterceptionPlan) {
        self.0
            .show_policy_block(plan.assessment(), plan.explanation());
    }

    fn show_intrinsic_block(&mut self, plan: &InterceptionPlan) {
        self.0
            .show_intrinsic_block(plan.assessment(), plan.explanation(), &[]);
    }

    fn warn(&mut self, message: &str) {
        self.0.warn(message);
    }
}

struct ShellRun<'a, R> {
    cmd: &'a str,
    prepared: &'a PreparedPlanner,
    plan: &'a InterceptionPlan,
    launch: &'a ShellLaunchOptions,
    sandbox_config: Option<&'a aegis_sandbox::SandboxConfig>,
    renderer: &'a R,
}

pub(crate) fn run_planned_shell_command(
    cmd: &str,
    prepared: &PreparedPlanner,
    plan: &InterceptionPlan,
    launch: &ShellLaunchOptions,
) -> i32 {
    run_planned_shell_command_with_renderer(cmd, prepared, plan, launch, &TerminalRenderer)
}

pub(crate) fn run_planned_shell_command_with_renderer<R: ExecutionRenderer>(
    cmd: &str,
    prepared: &PreparedPlanner,
    plan: &InterceptionPlan,
    launch: &ShellLaunchOptions,
    renderer: &R,
) -> i32 {
    let sandbox_config = match prepared {
        PreparedPlanner::Ready(context) => context.config().sandbox.as_ref(),
        PreparedPlanner::SetupFailure(_) => None,
    };
    run_planned_shell_command_with_sandbox(cmd, prepared, plan, launch, sandbox_config, renderer)
}

fn run_planned_shell_command_with_sandbox<R: ExecutionRenderer>(
    cmd: &str,
    prepared: &PreparedPlanner,
    plan: &InterceptionPlan,
    launch: &ShellLaunchOptions,
    sandbox_config: Option<&aegis_sandbox::SandboxConfig>,
    renderer: &R,
) -> i32 {
    ShellRun {
        cmd,
        prepared,
        plan,
        launch,
        sandbox_config,
        renderer,
    }
    .run()
}

impl<R: ExecutionRenderer> ShellRun<'_, R> {
    fn run(&self) -> i32 {
        let verdict = decide(self.cmd, self.plan, &mut ShellOutput(self.renderer));
        match verdict.decision {
            Decision::Denied => self.refuse(Decision::Denied, EXIT_DENIED),
            Decision::Blocked => self.refuse(Decision::Blocked, EXIT_BLOCKED),
            _ => self.execute(verdict),
        }
    }

    fn refuse(&self, decision: Decision, exit_code: i32) -> i32 {
        let status = aegis::runtime::sandbox_status_before_preparation(self.sandbox_config);
        match self.audit(&AuditRecord::without_snapshots(decision, status)) {
            Ok(()) => exit_code,
            Err(err) => {
                err!("error: failed to write audit log: {err}");
                EXIT_INTERNAL
            }
        }
    }

    fn execute(&self, verdict: aegis::runtime::disposition::Verdict) -> i32 {
        let coverage = self.create_snapshots();
        let attempts = coverage.snapshot_attempts();
        let gate = recovery_gate(
            self.cmd,
            self.plan,
            &coverage,
            verdict,
            &mut ShellOutput(self.renderer),
        );
        match gate {
            RecoveryGate::Deny(degradation) => {
                let record = AuditRecord {
                    decision: Decision::Denied,
                    sandbox_status: aegis::runtime::sandbox_status_before_preparation(
                        self.sandbox_config,
                    ),
                    snapshots: &coverage.records,
                    attempts: &attempts,
                    recovery_degradation: Some(degradation),
                };
                match self.audit(&record) {
                    Ok(()) => EXIT_DENIED,
                    Err(err) => {
                        err!("error: failed to write audit log: {err}");
                        EXIT_INTERNAL
                    }
                }
            }
            RecoveryGate::Run {
                decision,
                degradation,
            } => complete_shell_execution(
                decision,
                || prepare_command(self.cmd, self.launch, self.sandbox_config),
                |final_decision, sandbox_status| {
                    self.audit(&AuditRecord {
                        decision: final_decision,
                        sandbox_status,
                        snapshots: &coverage.records,
                        attempts: &attempts,
                        recovery_degradation: degradation,
                    })
                },
                exec_prepared_command,
                crate::shell_compat::wait_for_child,
                |message| self.renderer.report_error(message),
                || {
                    aegis::runtime::sandbox_unavailable_code_and_message(
                        aegis_sandbox::wsl1_unavailable(),
                        aegis_sandbox::apparmor_userns_restricted(),
                    )
                    .1
                },
            ),
        }
    }

    fn audit(&self, record: &AuditRecord<'_>) -> Result<(), AegisError> {
        match self.prepared {
            PreparedPlanner::Ready(context) => append_audit(context, self.plan, record, None),
            PreparedPlanner::SetupFailure(_) => Ok(()),
        }
    }

    fn create_snapshots(&self) -> SnapshotCoverage {
        let (PreparedPlanner::Ready(context), Some(request)) =
            (self.prepared, snapshot_request(self.plan))
        else {
            return SnapshotCoverage::default();
        };
        context.create_snapshots_for_plan(request.cwd, request.command, &request.git_targets)
    }
}

fn complete_shell_execution<C, X, AuditError>(
    decision: Decision,
    prepare: impl FnOnce() -> Result<(C, SandboxStatus), aegis_sandbox::SandboxError>,
    append_audit: impl FnOnce(Decision, SandboxStatus) -> Result<(), AuditError>,
    execute: impl FnOnce(C) -> Result<(X, SandboxStatus), aegis_sandbox::SandboxError>,
    wait: impl FnOnce(X) -> i32,
    report_error: impl FnOnce(&str),
    unavailable_message: impl FnOnce() -> &'static str,
) -> i32
where
    AuditError: std::fmt::Display,
{
    match prepare() {
        Ok((command, prep_status)) => {
            let needs_inner_report =
                cfg!(target_os = "linux") && prep_status == SandboxStatus::Active;
            if !needs_inner_report {
                if let Err(err) = append_audit(decision, prep_status) {
                    report_error(&format!("failed to write audit log: {err}"));
                    return EXIT_INTERNAL;
                }
                return match execute(command) {
                    Ok((child, _)) => wait(child),
                    Err(err) => {
                        report_error(&setup_failed_message(&err));
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
                        report_error(&setup_failed_message(
                            &"inner sandbox wrapper did not report status",
                        ));
                        return EXIT_INTERNAL;
                    }
                    if let Err(err) = append_audit(decision, actual_status) {
                        report_error(&format!("failed to write audit log: {err}"));
                        return EXIT_INTERNAL;
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
                    report_error(&setup_failed_message(&err));
                    EXIT_INTERNAL
                }
            }
        }
        Err(err) => match SandboxFailure::record(&err, append_audit) {
            Ok(failure) => {
                report_error(&failure.message(unavailable_message));
                failure.exit_code()
            }
            Err(audit_err) => {
                report_error(&format!("failed to write audit log: {audit_err}"));
                EXIT_INTERNAL
            }
        },
    }
}

#[cfg(test)]
mod sandbox_lifecycle_tests;
#[cfg(test)]
mod snapshot_ordering_tests;
