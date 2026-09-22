//! Human-readable explanation generation for decisions.

pub mod formatter;

pub use aegis_explanation::{
    AllowlistExplanation, CommandExplanation, ExecutionContextExplanation,
    ExecutionDecisionExplanation, ExecutionOutcomeExplanation, ExplainedPatternMatch,
    PolicyExplanation, ScanExplanation, SnapshotOutcomeExplanation,
};
