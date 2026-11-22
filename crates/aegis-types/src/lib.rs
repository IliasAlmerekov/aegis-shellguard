#![deny(missing_docs)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable
    )
)]

//! Core data types shared across the Aegis crates.
//!
//! This crate is the foundation of the dependency DAG: it carries the pure
//! data vocabulary — risk levels, the unified pattern representation, and the
//! human decision outcome — with no dependencies on any other Aegis crate.
//! Logic that *produces* these types (the scanner, parser, policy engine, audit
//! logger) lives in the crates that depend on this one.
//!
//! Exit-code contract for Aegis-generated outcomes:
//!
//! | Code | Meaning |
//! |------|---------|
//! | 0 | Success. |
//! | 1 | Snapshot absent for `snapshot verify` or `rollback` (ADR-052). |
//! | 2 | Denied or invalid invocation; rollback may refuse an inconclusive check. |
//! | 3 | Blocked. |
//! | 4 | Aegis or configuration error. |
//! | 5 | Snapshot verification was inconclusive and none was absent. |
//!
//! A launched command's exit code is passed through, including these values.

mod exits {
    /// The user denied the command at the confirmation dialog.
    pub const EXIT_DENIED: i32 = 2;
    /// The command matched a `Block`-level pattern and was hard-stopped.
    pub const EXIT_BLOCKED: i32 = 3;
    /// Aegis or configuration failure prevented execution or validation.
    pub const EXIT_INTERNAL: i32 = 4;
    /// A Snapshot liveness check found the artifact absent.
    pub const EXIT_ABSENT: i32 = 1;
    /// Snapshot verification was inconclusive and none was absent.
    pub const EXIT_INCONCLUSIVE: i32 = 5;
    /// The invocation is invalid.
    pub const EXIT_USAGE: i32 = 2;
}

mod analysis;
mod assessment;
mod command;
mod decision;
mod pattern;
mod policy;
mod recovery;
mod risk;
mod sandbox;
mod snapshot;

pub use analysis::classifier::{Classification, classify, language_match};
pub use analysis::{
    AnalysisProvenance, AnalysisStatus, AnalysisSummary, ByteSpan, DegradationReason,
    DetectedOperation, DetectionMechanism, DetectionSource, LanguageAnalysisResult, MatchEvidence,
    OperandCertainty, OperationKind, OperationModifiers, SourceOrigin, TargetAnalysis,
    merge_analysis,
};
pub use assessment::{Assessment, AssessmentBasis, DecisionSource, HighlightRange, MatchResult};
pub use command::{InlineScript, ParsedCommand};
pub use decision::Decision;
pub use exits::{
    EXIT_ABSENT, EXIT_BLOCKED, EXIT_DENIED, EXIT_INCONCLUSIVE, EXIT_INTERNAL, EXIT_USAGE,
};
pub use pattern::{Category, Pattern, PatternSource, PatternToken, PrefixPattern};
pub use policy::{AllowlistOverrideLevel, CiPolicy, Mode, PolicyRuleDecision, SnapshotPolicy};
pub use recovery::{
    RecoveryDegradation, SnapshotAttempt, SnapshotAttemptReadiness, StructuralCheck,
    ValidationLevel,
};
pub use risk::RiskLevel;
pub use sandbox::{CeilingLayer, SandboxStatus};
pub use snapshot::{RecordedSnapshot, SnapshotRecord};

#[cfg(test)]
mod tests {
    /// SnapshotRecord must be re-exported from aegis_types so that aegis-tui
    /// can import it without depending on the root aegis crate.
    /// This test fails until SnapshotRecord is added to this crate and
    /// re-exported from lib.rs.
    #[test]
    fn test_snapshot_record_exported_from_aegis_types() {
        use crate::SnapshotRecord;
        let record = SnapshotRecord {
            plugin: "git",
            snapshot_id: "stash-ref-abc123".to_string(),
            git_target: None,
        };
        assert_eq!(record.plugin, "git");
    }
}
