mod exits {
    pub const EXIT_DENIED: i32 = 2;
    pub const EXIT_BLOCKED: i32 = 3;
    pub const EXIT_INTERNAL: i32 = 4;
    pub const EXIT_ABSENT: i32 = 1;
    pub const EXIT_INCONCLUSIVE: i32 = 5;
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

pub use analysis::{
    AnalysisProvenance, AnalysisStatus, AnalysisSummary, ByteSpan, DegradationReason,
    DetectedOperation, DetectionMechanism, DetectionSource, MatchEvidence, OperandCertainty,
    OperationKind, OperationModifiers, SourceOrigin, TargetAnalysis,
};
pub use assessment::{
    Assessment, AssessmentBasis, DecisionSource, HighlightRange, LANGUAGE_AWARE_MATCH_LABEL,
    MatchResult,
};
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
