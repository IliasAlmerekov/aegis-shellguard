use std::path::PathBuf;

use tokio::io::BufReader as TokioBufReader;

use crate::planning::disposition::{
    DispositionOutput, RecoveryGate, decide, persist_allow_rule, recovery_gate, snapshot_request,
};
use crate::planning::{
    CwdState, InterceptionPlan, PlanningOutcome, PreparedPlanner, SetupFailurePlan,
};
use crate::runtime::{RuntimeContext, sandbox_status_before_preparation};
use aegis_policy::ExecutionTransport;
use aegis_tui::{
    PromptDecision, RecoveryPromptDecision, show_block_via_tty,
    show_confirmation_via_tty_with_decision, show_policy_block_via_tty,
    show_recovery_override_via_tty,
};
use aegis_types::{Decision, EXIT_BLOCKED, EXIT_DENIED, EXIT_INTERNAL};

use super::exec::execute_prepared_and_emit;
use super::protocol::{
    InputFrame, MAX_FRAME_BYTES, OutputDecision, ReadLineResult, emit_error_or_exit,
    emit_result_or_exit, exit_internal, read_bounded_line,
};
use super::sandbox::{
    WatchExecution, append_watch_execution_audit, complete_watch_approved_execution,
    emit_watch_audit_error, prepare_watch_command, warn_if_sandbox_unavailable_at_startup,
};

pub async fn run(prepared: &PreparedPlanner, ci_detected: bool) -> i32 {
    if !ci_detected {
        match crate::toggle::status() {
            Ok(crate::toggle::ToggleState::Disabled) => return run_disabled().await,
            Ok(crate::toggle::ToggleState::Enabled) => {}
            Err(err) => {
                err!("error: failed to read toggle state: {err}");
                return EXIT_INTERNAL;
            }
        }
    }

    let context = match prepared {
        PreparedPlanner::Ready(context) => context,
        PreparedPlanner::SetupFailure(plan) => {
            report_watch_setup_failure(plan);
            return EXIT_INTERNAL;
        }
    };

    warn_if_sandbox_unavailable_at_startup(context).await;

    let mut reader = TokioBufReader::new(tokio::io::stdin());

    loop {
        match read_bounded_line(&mut reader, MAX_FRAME_BYTES).await {
            Err(e) => {
                err!("aegis: stdin read error: {e}");
                return EXIT_INTERNAL;
            }
            Ok(ReadLineResult::Eof) => return 0,
            Ok(ReadLineResult::Oversized) => {
                emit_error_or_exit(None, "frame exceeds 1 MiB limit".to_string());
            }
            Ok(ReadLineResult::Line(line)) => {
                if line.trim().is_empty() {
                    continue;
                }
                process_frame(line, context, ci_detected).await;
            }
        }
    }
}

pub async fn run_disabled() -> i32 {
    let mut reader = TokioBufReader::new(tokio::io::stdin());

    loop {
        match read_bounded_line(&mut reader, MAX_FRAME_BYTES).await {
            Err(e) => {
                err!("aegis: stdin read error: {e}");
                return EXIT_INTERNAL;
            }
            Ok(ReadLineResult::Eof) => return 0,
            Ok(ReadLineResult::Oversized) => {
                emit_error_or_exit(None, "frame exceeds 1 MiB limit".to_string());
            }
            Ok(ReadLineResult::Line(line)) => {
                if line.trim().is_empty() {
                    continue;
                }
                process_disabled_frame(line).await;
            }
        }
    }
}

async fn process_frame(line: String, context: &RuntimeContext, ci_detected: bool) {
    let frame: InputFrame = match serde_json::from_str(&line) {
        Ok(f) => f,
        Err(e) => {
            let msg = format!("invalid JSON: {e}");
            emit_error_or_exit(None, msg);
            return;
        }
    };

    let id = frame.id.clone();

    if frame.cmd.trim().is_empty() {
        emit_error_or_exit(id.clone(), "missing or empty cmd".to_string());
        return;
    }

    let cwd_state = if let Some(ref cwd_str) = frame.cwd {
        let path = PathBuf::from(cwd_str);
        if !path.is_dir() {
            emit_error_or_exit(id.clone(), "invalid cwd".to_string());
            return;
        }
        CwdState::Resolved(path)
    } else {
        match std::env::current_dir() {
            Ok(path) => CwdState::Resolved(path),
            Err(_) => CwdState::Unavailable,
        }
    };
    let outcome = crate::planning::plan_with_context_async(
        context,
        crate::planning::PlanningRequest {
            command: &frame.cmd,
            cwd_state,
            transport: ExecutionTransport::Watch,
            ci_detected,
        },
    )
    .await;

    match outcome {
        PlanningOutcome::SetupFailure(plan) => {
            report_watch_setup_failure(&plan);
            exit_internal();
        }
        PlanningOutcome::Planned(plan) => run_watch_plan(frame, context, plan).await,
    }
}

async fn process_disabled_frame(line: String) {
    let frame: InputFrame = match serde_json::from_str(&line) {
        Ok(f) => f,
        Err(e) => {
            let msg = format!("invalid JSON: {e}");
            emit_error_or_exit(None, msg);
            return;
        }
    };

    let id = frame.id.clone();

    if frame.cmd.trim().is_empty() {
        emit_error_or_exit(id, "missing or empty cmd".to_string());
        return;
    }

    let cwd = match resolve_frame_cwd(&frame, &id) {
        Ok(path) => path,
        Err(()) => return,
    };

    execute_and_emit(&frame.cmd, &cwd, id).await;
}

async fn run_watch_plan(frame: InputFrame, context: &RuntimeContext, plan: InterceptionPlan) {
    run_watch_plan_with_prompts(
        frame,
        context,
        plan,
        |assessment, explanation| {
            tokio::task::block_in_place(|| {
                show_confirmation_via_tty_with_decision(assessment, explanation, &[])
            })
        },
        |degradation| tokio::task::block_in_place(|| show_recovery_override_via_tty(degradation)),
    )
    .await;
}

async fn run_watch_plan_with_prompts<C, R>(
    frame: InputFrame,
    context: &RuntimeContext,
    plan: InterceptionPlan,
    confirmation_prompt: C,
    recovery_prompt: R,
) where
    C: FnMut(&aegis_types::Assessment, &aegis_explanation::CommandExplanation) -> PromptDecision,
    R: FnMut(aegis_types::RecoveryDegradation) -> RecoveryPromptDecision,
{
    run_watch_plan_with_persistence(
        frame,
        context,
        plan,
        confirmation_prompt,
        recovery_prompt,
        persist_allow_rule,
    )
    .await;
}

struct WatchOutput<C, R, P> {
    confirmation_prompt: C,
    recovery_prompt: R,
    persist_allow: P,
}

impl<C, R, P> DispositionOutput for WatchOutput<C, R, P>
where
    C: FnMut(&aegis_types::Assessment, &aegis_explanation::CommandExplanation) -> PromptDecision,
    R: FnMut(aegis_types::RecoveryDegradation) -> RecoveryPromptDecision,
    P: FnMut(&str, &std::path::Path),
{
    fn confirm(&mut self, plan: &InterceptionPlan) -> PromptDecision {
        (self.confirmation_prompt)(plan.assessment(), plan.explanation())
    }

    fn recovery_override(
        &mut self,
        degradation: aegis_types::RecoveryDegradation,
    ) -> RecoveryPromptDecision {
        (self.recovery_prompt)(degradation)
    }

    fn show_policy_block(&mut self, plan: &InterceptionPlan) {
        tokio::task::block_in_place(|| {
            show_policy_block_via_tty(plan.assessment(), plan.explanation());
        });
    }

    fn show_intrinsic_block(&mut self, plan: &InterceptionPlan) {
        tokio::task::block_in_place(|| show_block_via_tty(plan.assessment(), plan.explanation()));
    }

    fn warn(&mut self, message: &str) {
        err!("warning: {message}");
    }

    fn persist_allow(&mut self, cmd: &str, cwd: &std::path::Path) {
        (self.persist_allow)(cmd, cwd);
    }
}

async fn run_watch_plan_with_persistence<C, R, P>(
    frame: InputFrame,
    context: &RuntimeContext,
    plan: InterceptionPlan,
    confirmation_prompt: C,
    recovery_prompt: R,
    persist_allow: P,
) where
    C: FnMut(&aegis_types::Assessment, &aegis_explanation::CommandExplanation) -> PromptDecision,
    R: FnMut(aegis_types::RecoveryDegradation) -> RecoveryPromptDecision,
    P: FnMut(&str, &std::path::Path),
{
    let id = frame.id.clone();
    let cwd = plan.decision_context().cwd_state().path();
    let mut out = WatchOutput {
        confirmation_prompt,
        recovery_prompt,
        persist_allow,
    };

    let verdict = decide(&frame.cmd, &plan, &mut out);
    let refused = |decision: Decision, output: OutputDecision, exit_code: i32| {
        let execution = WatchExecution {
            frame: &frame,
            context,
            plan: &plan,
            cwd,
            snapshots: &[],
            attempts: &[],
            recovery_degradation: None,
        };
        let status = sandbox_status_before_preparation(context.config().sandbox.as_ref());
        match append_watch_execution_audit(&execution, decision, status) {
            Ok(()) => emit_result_or_exit(execution.frame.id.clone(), output, exit_code),
            Err(err) => emit_watch_audit_error(&execution.frame.id, &err),
        }
    };
    match verdict.decision {
        Decision::Denied => refused(Decision::Denied, OutputDecision::Denied, EXIT_DENIED),
        Decision::Blocked => refused(Decision::Blocked, OutputDecision::Blocked, EXIT_BLOCKED),
        _ => {
            let coverage = match snapshot_request(&plan) {
                Some(request) => {
                    context
                        .create_snapshots_for_plan_async(
                            request.cwd,
                            request.command,
                            request.git_targets.process_cwd,
                            request.git_targets.resolved_paths(),
                        )
                        .await
                }
                None => aegis_snapshot::SnapshotCoverage::default(),
            };
            let attempts = coverage.snapshot_attempts();
            let execution = |recovery_degradation| WatchExecution {
                frame: &frame,
                context,
                plan: &plan,
                cwd,
                snapshots: coverage.records.as_slice(),
                attempts: attempts.as_slice(),
                recovery_degradation,
            };
            match recovery_gate(&frame.cmd, &plan, &coverage, verdict, &mut out) {
                RecoveryGate::Deny(degradation) => {
                    let status =
                        sandbox_status_before_preparation(context.config().sandbox.as_ref());
                    match append_watch_execution_audit(
                        &execution(Some(degradation)),
                        Decision::Denied,
                        status,
                    ) {
                        Ok(()) => emit_result_or_exit(id, OutputDecision::Denied, EXIT_DENIED),
                        Err(err) => emit_watch_audit_error(&id, &err),
                    }
                }
                RecoveryGate::Run {
                    decision,
                    degradation,
                } => complete_watch_approved_execution(execution(degradation), decision).await,
            }
        }
    }
}

fn resolve_frame_cwd(frame: &InputFrame, id: &Option<String>) -> Result<PathBuf, ()> {
    if let Some(ref cwd_str) = frame.cwd {
        let path = PathBuf::from(cwd_str);
        if !path.is_dir() {
            emit_error_or_exit(id.clone(), "invalid cwd".to_string());
            return Err(());
        }
        return Ok(path);
    }

    std::env::current_dir().map_err(|_| {
        emit_error_or_exit(id.clone(), "cwd unavailable".to_string());
    })
}

fn report_watch_setup_failure(plan: &SetupFailurePlan) {
    err!("{}", plan.user_message());
    if plan.is_config_fault() {
        err!("error: Fix or remove the invalid config file and try again.");
    }
}

async fn execute_and_emit(cmd: &str, cwd: &std::path::Path, id: Option<String>) {
    let (command, _) = match prepare_watch_command(cmd).await {
        Ok(prepared) => prepared,
        Err(err) => {
            emit_error_or_exit(id, format!("failed to prepare child: {err}"));
            return;
        }
    };
    execute_prepared_and_emit(command, Some(cwd), id).await;
}

#[cfg(test)]
mod tests;
