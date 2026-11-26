use serde::{Deserialize, Serialize};

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryDegradation {
    NoSnapshotAvailable,
    PartialSnapshotCoverage,
    UnresolvedGitTarget,
    UnavailableCwd,
    SnapshotArtifactUnavailable,
    SnapshotArtifactInvalid,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotAttemptReadiness {
    Ready,
    Invalid,
    Unavailable,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationLevel {
    PresenceOnly,
    Structural,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuralCheck {
    Passed,
    ToolUnavailable,
    TimedOut,
    NotSupported,
    NotRun,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotAttempt {
    pub plugin: String,
    pub readiness: SnapshotAttemptReadiness,
    pub validation_level: ValidationLevel,
    pub structural_check: StructuralCheck,
}
