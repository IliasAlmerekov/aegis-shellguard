//! Recovery backstop degradation recorded in the audit log (ADR-016).

use serde::{Deserialize, Serialize};

/// Why a required recovery backstop was not available for an
/// bounded `Effect-opaque execution`.
///
/// Recorded in the audit log so a degraded recovery is a first-class,
/// queryable event — distinct from `SnapshotPolicy::None`, which is a trusted
/// global opt-out, not a degradation. Orthogonal to `RiskLevel`: an
/// effect-opaque command can remain `Safe` while still requiring a snapshot,
/// and failing to create that snapshot is a degradation even though the risk
/// axis never moved.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryDegradation {
    /// A Snapshot was required for bounded effect-opaque execution under
    /// `SnapshotPolicy::Selective` / `Full`, but none could be created before
    /// execution. Non-interactive execution fails closed; interactive execution
    /// must surface this reason.
    NoSnapshotAvailable,
    /// A Snapshot was required for bounded effect-opaque execution, at least
    /// one applicable `Snapshot plugin` produced a record, and fewer records
    /// came back than there were applicable plugins: an applicable plugin
    /// failed. Coverage is incomplete, and Aegis cannot tell whether the
    /// plugin that failed covered state the command will touch.
    PartialSnapshotCoverage,
    /// An unattended run needs a git Snapshot of the working tree a `git`
    /// command acts on, but that Git target could not be resolved (a variable
    /// or glob path, `GIT_DIR`, a missing path). Nothing covers it, and Aegis
    /// does not substitute a Snapshot of the process cwd (#471).
    UnresolvedGitTarget,
    /// A required Snapshot was recorded, but its artifact is gone: the
    /// Snapshot cannot be restored (ADR-031 §8).
    SnapshotArtifactUnavailable,
    /// A required Snapshot was recorded, but its artifact failed validation:
    /// what it holds cannot be trusted for a restore (ADR-031 §8).
    SnapshotArtifactInvalid,
}

/// Whether one `Snapshot plugin` attempt left a usable Snapshot (ADR-031 §10).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotAttemptReadiness {
    /// The artifact exists and passed the validation that ran.
    Ready,
    /// The artifact exists but failed validation.
    Invalid,
    /// No artifact exists, or its presence could not be established.
    Unavailable,
}

/// How deeply a Snapshot artifact was validated (ADR-031 §10).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationLevel {
    /// Only the artifact's presence was checked.
    PresenceOnly,
    /// The artifact's structure was checked as well.
    Structural,
}

/// Outcome of the structural check of a Snapshot artifact (ADR-031 §10).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuralCheck {
    /// The structural check ran and passed.
    Passed,
    /// The tool the check needs is not installed.
    ToolUnavailable,
    /// The check ran past its time limit.
    TimedOut,
    /// The plugin has no structural check.
    NotSupported,
}

/// One `Snapshot plugin` attempt and what became of its artifact, recorded in
/// the audit log per attempt actually made (ADR-031 §8, §10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotAttempt {
    /// Name of the `Snapshot plugin`, as in `SnapshotRecord::plugin`.
    pub plugin: String,
    /// Whether the attempt left a usable Snapshot.
    pub readiness: SnapshotAttemptReadiness,
    /// How deeply the artifact was validated.
    pub validation_level: ValidationLevel,
    /// Outcome of the structural check.
    pub structural_check: StructuralCheck,
}
