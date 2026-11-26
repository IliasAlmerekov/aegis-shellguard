use std::borrow::Cow;
use std::path::Path;

use aegis_config::allowlist::ConfigSourceLayer;
use aegis_config::amend::{
    AppendOutcome, active_config_path_for_append, append_allow_rule, append_block_rule,
};
use aegis_parser::{extract_prefix, split_tokens};
use aegis_policy::BlockReason;
use aegis_snapshot::SnapshotCoverage;
use aegis_tui::{PromptDecision, RecoveryPromptDecision};
use aegis_types::{
    Decision, EXIT_BLOCKED, EXIT_INTERNAL, RecoveryDegradation, SandboxStatus, SnapshotAttempt,
    SnapshotRecord,
};

use super::{
    AuditWriteOptions, RuntimeContext, WatchAuditContext, allow_rule_persistence_degradation,
    plan_recovery_degradation,
};
use crate::error::AegisError;
use crate::planning::{
    CwdState, ExecutionDisposition, GitTargetPlan, InterceptionPlan, SnapshotPlan,
};

pub trait DispositionOutput {
    fn confirm(&mut self, plan: &InterceptionPlan) -> PromptDecision;

    fn recovery_override(&mut self, degradation: RecoveryDegradation) -> RecoveryPromptDecision;

    fn show_policy_block(&mut self, plan: &InterceptionPlan);

    fn show_intrinsic_block(&mut self, plan: &InterceptionPlan);

    fn warn(&mut self, message: &str);

    fn persist_allow(&mut self, cmd: &str, cwd: &Path);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Verdict {
    pub decision: Decision,
    pub approve_always: bool,
}

pub fn decide(cmd: &str, plan: &InterceptionPlan, out: &mut impl DispositionOutput) -> Verdict {
    match plan.execution_disposition() {
        ExecutionDisposition::Execute => Verdict {
            decision: Decision::AutoApproved,
            approve_always: false,
        },
        ExecutionDisposition::RequiresApproval => {
            let prompt = out.confirm(plan);
            if prompt == PromptDecision::DenyAlways {
                persist_block_rule(cmd, plan);
            }
            let approved = matches!(
                prompt,
                PromptDecision::Approve | PromptDecision::ApproveAlways
            );
            Verdict {
                decision: if approved {
                    Decision::Approved
                } else {
                    Decision::Denied
                },
                approve_always: prompt == PromptDecision::ApproveAlways,
            }
        }
        ExecutionDisposition::Block => {
            show_block(plan, out);
            Verdict {
                decision: Decision::Blocked,
                approve_always: false,
            }
        }
    }
}

fn show_block(plan: &InterceptionPlan, out: &mut impl DispositionOutput) {
    match plan.policy_decision().block_reason() {
        Some(BlockReason::IntrinsicRiskBlock) => out.show_intrinsic_block(plan),
        Some(
            BlockReason::ProtectCiPolicy
            | BlockReason::StrictPolicy
            | BlockReason::BlocklistOverride
            | BlockReason::PolicyRulesOverride,
        ) => out.show_policy_block(plan),
        None => {}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryGate {
    Run {
        decision: Decision,
        degradation: Option<RecoveryDegradation>,
    },
    Deny(RecoveryDegradation),
}

pub fn recovery_gate(
    cmd: &str,
    plan: &InterceptionPlan,
    coverage: &SnapshotCoverage,
    verdict: Verdict,
    out: &mut impl DispositionOutput,
) -> RecoveryGate {
    let Verdict {
        decision,
        approve_always,
    } = verdict;
    let cwd = match plan.decision_context().cwd_state() {
        CwdState::Resolved(path) => Some(path.as_path()),
        CwdState::Unavailable => None,
    };
    if approve_always && cwd.is_none() {
        err!("warning: cannot persist allow rule: working directory unavailable");
    }
    let approve_always =
        approve_always && cwd.is_some() && !plan.explanation().context.recovery_opt_out;
    let persistence_block = if approve_always {
        allow_rule_persistence_degradation(plan, coverage)
    } else {
        None
    };
    if persistence_block.is_some() {
        out.warn("allow rule not saved: Recovery is not ready for this run");
    }
    let degradation = plan_recovery_degradation(plan, decision == Decision::AutoApproved, coverage)
        .or(persistence_block);
    if approve_always
        && degradation.is_none()
        && let Some(cwd) = cwd
    {
        out.persist_allow(cmd, cwd);
    }
    let Some(degradation) = degradation else {
        return RecoveryGate::Run {
            decision,
            degradation: None,
        };
    };
    match out.recovery_override(degradation) {
        RecoveryPromptDecision::RunOnceWithoutRecovery => RecoveryGate::Run {
            decision: Decision::Approved,
            degradation: Some(degradation),
        },
        RecoveryPromptDecision::Deny => RecoveryGate::Deny(degradation),
    }
}

pub struct SnapshotRequest<'a> {
    pub cwd: &'a Path,
    pub command: &'a str,
    pub git_targets: GitTargetPlan,
}

pub fn snapshot_request(plan: &InterceptionPlan) -> Option<SnapshotRequest<'_>> {
    let SnapshotPlan::Required { git_targets, .. } = plan.snapshot_plan() else {
        return None;
    };
    let CwdState::Resolved(cwd) = plan.decision_context().cwd_state() else {
        return None;
    };
    Some(SnapshotRequest {
        cwd,
        command: &plan.assessment().command.raw,
        git_targets,
    })
}

#[derive(Debug, Clone, Copy)]
enum RuleKind {
    Allow,
    Block,
}

impl RuleKind {
    fn label(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Block => "block",
        }
    }
}

pub fn persist_allow_rule(cmd: &str, cwd: &Path) {
    persist_rule(RuleKind::Allow, cmd, cwd);
}

fn persist_block_rule(cmd: &str, plan: &InterceptionPlan) {
    let CwdState::Resolved(cwd) = plan.decision_context().cwd_state() else {
        err!("warning: cannot persist block rule: working directory unavailable");
        return;
    };
    persist_rule(RuleKind::Block, cmd, cwd);
}

fn persist_rule(kind: RuleKind, cmd: &str, cwd: &Path) {
    let label = kind.label();
    let Some(config_path) = active_config_path_for_append() else {
        err!("warning: cannot persist {label} rule: no config file found");
        return;
    };
    let tokens = split_tokens(cmd);
    let prefix = extract_prefix(&tokens);
    let outcome = match kind {
        RuleKind::Allow => append_allow_rule(&config_path, &prefix, cwd),
        RuleKind::Block => append_block_rule(&config_path, &prefix, cwd),
    };
    match outcome {
        Ok(AppendOutcome::Conflict {
            pattern,
            existing_location,
        }) => {
            let location = match existing_location {
                ConfigSourceLayer::Project => "project",
                ConfigSourceLayer::Global => "global",
            };
            err!("warning: conflicting rule for '{pattern}' already exists in {location} config");
        }
        Ok(AppendOutcome::SkippedDuplicate | AppendOutcome::Appended) => {}
        Err(err) => err!("error: failed to append {label} rule: {err}"),
    }
}

pub struct AuditRecord<'a> {
    pub decision: Decision,
    pub sandbox_status: SandboxStatus,
    pub snapshots: &'a [SnapshotRecord],
    pub attempts: &'a [SnapshotAttempt],
    pub recovery_degradation: Option<RecoveryDegradation>,
}

impl AuditRecord<'_> {
    pub fn without_snapshots(decision: Decision, sandbox_status: SandboxStatus) -> Self {
        Self {
            decision,
            sandbox_status,
            snapshots: &[],
            attempts: &[],
            recovery_degradation: None,
        }
    }
}

pub struct WatchOrigin {
    pub source: Option<String>,
    pub cwd: Option<String>,
    pub id: Option<String>,
}

pub fn append_audit(
    context: &RuntimeContext,
    plan: &InterceptionPlan,
    record: &AuditRecord<'_>,
    origin: Option<WatchOrigin>,
) -> Result<(), AegisError> {
    let allowlist_match = plan.decision_context().allowlist_match();
    let allowlist_effective = plan.policy_decision().allowlist_effective;
    let ci_detected = plan.decision_context().ci_detected();
    let (assessment, explanation) = (plan.assessment(), plan.explanation());
    let AuditRecord {
        decision,
        sandbox_status,
        snapshots,
        attempts: snapshot_attempts,
        recovery_degradation,
    } = *record;
    match (origin, recovery_degradation) {
        (None, None) => context.append_audit_entry(
            assessment,
            decision,
            snapshots,
            explanation,
            AuditWriteOptions {
                allowlist_match,
                allowlist_effective,
                ci_detected,
                sandbox_status,
                snapshot_attempts,
            },
        ),
        (None, Some(degradation)) => context.append_audit_entry_with_recovery_degradation(
            assessment,
            decision,
            snapshots,
            explanation,
            AuditWriteOptions {
                allowlist_match,
                allowlist_effective,
                ci_detected,
                sandbox_status,
                snapshot_attempts,
            },
            degradation,
        ),
        (Some(origin), None) => context.append_watch_audit_entry(
            assessment,
            decision,
            snapshots,
            explanation,
            WatchAuditContext {
                allowlist_match,
                allowlist_effective,
                ci_detected,
                sandbox_status,
                snapshot_attempts,
                source: origin.source,
                cwd: origin.cwd,
                id: origin.id,
            },
        ),
        (Some(origin), Some(degradation)) => context
            .append_watch_audit_entry_with_recovery_degradation(
                assessment,
                decision,
                snapshots,
                explanation,
                WatchAuditContext {
                    allowlist_match,
                    allowlist_effective,
                    ci_detected,
                    sandbox_status,
                    snapshot_attempts,
                    source: origin.source,
                    cwd: origin.cwd,
                    id: origin.id,
                },
                degradation,
            ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxFailure {
    RequiredBlocked,
    RequiredBlockedNested,
    SetupFailed(String),
}

impl SandboxFailure {
    pub fn record<E>(
        error: &aegis_sandbox::SandboxError,
        append_audit: impl FnOnce(Decision, SandboxStatus) -> Result<(), E>,
    ) -> Result<Self, E> {
        let failure = match error {
            aegis_sandbox::SandboxError::Required => Self::RequiredBlocked,
            aegis_sandbox::SandboxError::RequiredNestedUnderOuterSandbox => {
                Self::RequiredBlockedNested
            }
            other => Self::SetupFailed(other.to_string()),
        };
        append_audit(Decision::Blocked, failure.audit_status())?;
        Ok(failure)
    }

    fn audit_status(&self) -> SandboxStatus {
        match self {
            Self::RequiredBlocked | Self::RequiredBlockedNested => SandboxStatus::Unavailable,
            Self::SetupFailed(_) => SandboxStatus::NotAttempted,
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Self::RequiredBlocked | Self::RequiredBlockedNested => EXIT_BLOCKED,
            Self::SetupFailed(_) => EXIT_INTERNAL,
        }
    }

    pub fn message(&self, unavailable: impl FnOnce() -> &'static str) -> Cow<'static, str> {
        match self {
            Self::RequiredBlocked => Cow::Borrowed(unavailable()),
            Self::RequiredBlockedNested => {
                Cow::Borrowed(super::SANDBOX_REQUIRED_NESTED_UNAVAILABLE_MESSAGE)
            }
            Self::SetupFailed(err) => Cow::Owned(setup_failed_message(err)),
        }
    }
}

pub fn setup_failed_message(err: &impl std::fmt::Display) -> String {
    format!("Sandbox setup failed; command not executed: {err}")
}
