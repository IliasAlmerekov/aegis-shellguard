use std::sync::Mutex;

use aegis_explanation::CommandExplanation;
use aegis_types::{Assessment, RecoveryDegradation, SnapshotRecord};

use crate::{PromptDecision, RecoveryPromptDecision};

/// Output and prompt boundary used by plan executors.
pub trait ExecutionRenderer {
    /// Ask whether a planned command should run.
    fn show_confirmation(
        &self,
        assessment: &Assessment,
        explanation: &CommandExplanation,
        snapshots: &[SnapshotRecord],
    ) -> PromptDecision;

    /// Ask whether a command may run without the required recovery coverage.
    fn show_recovery_override(&self, degradation: RecoveryDegradation) -> RecoveryPromptDecision;

    /// Show a policy block.
    fn show_policy_block(&self, assessment: &Assessment, explanation: &CommandExplanation);

    /// Show a block caused by the command's intrinsic risk.
    fn show_intrinsic_block(
        &self,
        assessment: &Assessment,
        explanation: &CommandExplanation,
        snapshots: &[SnapshotRecord],
    );

    /// Emit a non-fatal execution warning.
    fn warn(&self, message: &str);

    /// Emit an execution error.
    fn report_error(&self, message: &str);
}

/// Renderer for the terminal used by the Wrapper transport.
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

    fn warn(&self, message: &str) {
        eprintln!("warning: {message}");
    }

    fn report_error(&self, message: &str) {
        eprintln!("error: {message}");
    }
}

/// A scripted renderer for execution tests.
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

    pub fn set_confirmation_decision(&self, decision: PromptDecision) {
        *self
            .confirmation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = decision;
    }

    pub fn set_recovery_decision(&self, decision: RecoveryPromptDecision) {
        *self
            .recovery
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
