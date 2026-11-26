use std::sync::Mutex;

use aegis_explanation::CommandExplanation;
use aegis_types::{Assessment, RecoveryDegradation, SnapshotRecord};

use crate::{PromptDecision, RecoveryPromptDecision};

pub trait ExecutionRenderer {
    fn show_confirmation(
        &self,
        assessment: &Assessment,
        explanation: &CommandExplanation,
        snapshots: &[SnapshotRecord],
    ) -> PromptDecision;

    fn show_recovery_override(&self, degradation: RecoveryDegradation) -> RecoveryPromptDecision;

    fn show_policy_block(&self, assessment: &Assessment, explanation: &CommandExplanation);

    fn show_intrinsic_block(
        &self,
        assessment: &Assessment,
        explanation: &CommandExplanation,
        snapshots: &[SnapshotRecord],
    );

    fn warn(&self, message: &str);

    fn report_error(&self, message: &str);
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TerminalRenderer;

impl ExecutionRenderer for TerminalRenderer {
    fn show_confirmation(
        &self,
        assessment: &Assessment,
        explanation: &CommandExplanation,
        snapshots: &[SnapshotRecord],
    ) -> PromptDecision {
        crate::show_confirmation_decision(assessment, explanation, snapshots)
    }

    fn show_recovery_override(&self, degradation: RecoveryDegradation) -> RecoveryPromptDecision {
        crate::show_recovery_override_decision(degradation)
    }

    fn show_policy_block(&self, assessment: &Assessment, explanation: &CommandExplanation) {
        crate::show_policy_block(assessment, explanation);
    }

    fn show_intrinsic_block(
        &self,
        assessment: &Assessment,
        explanation: &CommandExplanation,
        snapshots: &[SnapshotRecord],
    ) {
        crate::show_confirmation(assessment, explanation, snapshots);
    }

    #[expect(
        clippy::print_stderr,
        reason = "rendering module writes diagnostics to stderr"
    )]
    fn warn(&self, message: &str) {
        eprintln!("warning: {message}");
    }

    #[expect(
        clippy::print_stderr,
        reason = "rendering module writes diagnostics to stderr"
    )]
    fn report_error(&self, message: &str) {
        eprintln!("error: {message}");
    }
}

#[derive(Debug)]
pub struct TestRenderer {
    confirmation: Mutex<PromptDecision>,
    recovery: Mutex<RecoveryPromptDecision>,
    calls: Mutex<Vec<ExecutionRendererCall>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionRendererCall {
    Confirmation,
    RecoveryOverride(RecoveryDegradation),
    PolicyBlock,
    IntrinsicBlock,
    Warning(String),
    Error(String),
}

impl TestRenderer {
    pub fn new() -> Self {
        Self {
            confirmation: Mutex::new(PromptDecision::Deny),
            recovery: Mutex::new(RecoveryPromptDecision::Deny),
            calls: Mutex::new(Vec::new()),
        }
    }

    #[cfg(any(test, feature = "testing"))]
    pub fn set_confirmation_decision(&self, decision: PromptDecision) {
        *self
            .confirmation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = decision;
    }

    pub fn calls(&self) -> Vec<ExecutionRendererCall> {
        self.calls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl Default for TestRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionRenderer for TestRenderer {
    fn show_confirmation(
        &self,
        _assessment: &Assessment,
        _explanation: &CommandExplanation,
        _snapshots: &[SnapshotRecord],
    ) -> PromptDecision {
        self.calls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(ExecutionRendererCall::Confirmation);
        *self
            .confirmation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn show_recovery_override(&self, degradation: RecoveryDegradation) -> RecoveryPromptDecision {
        self.calls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(ExecutionRendererCall::RecoveryOverride(degradation));
        *self
            .recovery
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn show_policy_block(&self, _assessment: &Assessment, _explanation: &CommandExplanation) {
        self.calls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(ExecutionRendererCall::PolicyBlock);
    }

    fn show_intrinsic_block(
        &self,
        _assessment: &Assessment,
        _explanation: &CommandExplanation,
        _snapshots: &[SnapshotRecord],
    ) {
        self.calls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(ExecutionRendererCall::IntrinsicBlock);
    }

    fn warn(&self, message: &str) {
        self.calls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(ExecutionRendererCall::Warning(message.to_owned()));
    }

    fn report_error(&self, message: &str) {
        self.calls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(ExecutionRendererCall::Error(message.to_owned()));
    }
}
