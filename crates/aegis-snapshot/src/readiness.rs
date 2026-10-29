//! Snapshot attempt readiness: a local check of one attempt's artifact
//! (ADR-031 §6).

use std::path::Path;
use std::time::Duration;

use aegis_types::{SnapshotAttemptReadiness, StructuralCheck, ValidationLevel};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Upper bound for one attempt's whole check, presence included. The registry
/// enforces it; a presence check that overruns is `Unavailable` (ADR-031 §6).
pub(crate) const ATTEMPT_TIMEOUT: Duration = Duration::from_millis(100);

/// Upper bound for a provider's structural tool. It ends before
/// [`ATTEMPT_TIMEOUT`], so a slow tool reads as `TimedOut` (presence already
/// shown) and never as the registry's `Unavailable`.
pub(crate) const STRUCTURAL_TIMEOUT: Duration = Duration::from_millis(60);

/// The first 16 bytes of every SQLite database file.
const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";

/// What one provider's check of one artifact found: the per-attempt triple of
/// ADR-031 §6. The registry adds the plugin name to make a `SnapshotAttempt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtifactReadiness {
    /// Whether the attempt left a usable Snapshot.
    pub readiness: SnapshotAttemptReadiness,
    /// How deeply the artifact was validated.
    pub validation_level: ValidationLevel,
    /// Outcome of the structural check.
    pub structural_check: StructuralCheck,
}

impl ArtifactReadiness {
    /// Presence check failed: the artifact is missing, unreadable, or the
    /// attempt left no record. The structural check never ran.
    pub const UNAVAILABLE: Self = Self {
        readiness: SnapshotAttemptReadiness::Unavailable,
        validation_level: ValidationLevel::PresenceOnly,
        structural_check: StructuralCheck::NotRun,
    };

    /// Presence check failed: the artifact is empty.
    pub const EMPTY: Self = Self {
        readiness: SnapshotAttemptReadiness::Invalid,
        validation_level: ValidationLevel::PresenceOnly,
        structural_check: StructuralCheck::NotRun,
    };

    /// The artifact is present and the plugin has no structural check. Also
    /// the verdict for an attempt that recorded there was nothing to capture.
    // shortcut: `NotSupported` stands in for "no artifact to parse" on the
    // clean-tree and no-container sentinels, upgrade if the audit schema gains
    // a dedicated value.
    pub const NOT_SUPPORTED: Self = Self::present(StructuralCheck::NotSupported);

    /// The artifact is present; the structural check ended as `check`
    /// (anything but `Passed`, which needs a verdict).
    const fn present(check: StructuralCheck) -> Self {
        Self {
            readiness: SnapshotAttemptReadiness::Ready,
            validation_level: ValidationLevel::PresenceOnly,
            structural_check: check,
        }
    }

    /// The structural check ran and found the artifact sound.
    const STRUCTURAL: Self = Self {
        readiness: SnapshotAttemptReadiness::Ready,
        validation_level: ValidationLevel::Structural,
        structural_check: StructuralCheck::Passed,
    };

    /// The structural check ran and reported corruption.
    const CORRUPT: Self = Self {
        readiness: SnapshotAttemptReadiness::Invalid,
        validation_level: ValidationLevel::PresenceOnly,
        structural_check: StructuralCheck::Passed,
    };
}

/// Presence check of a local file: it opens, is a regular file, and is not
/// empty. Returns the open file so a structural check can read it.
async fn check_file_present(path: &Path) -> Result<tokio::fs::File, ArtifactReadiness> {
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|_| ArtifactReadiness::UNAVAILABLE)?;
    let metadata = file
        .metadata()
        .await
        .map_err(|_| ArtifactReadiness::UNAVAILABLE)?;
    if !metadata.is_file() {
        return Err(ArtifactReadiness::UNAVAILABLE);
    }
    if metadata.len() == 0 {
        return Err(ArtifactReadiness::EMPTY);
    }
    Ok(file)
}

/// Readiness of a local file with no structural check (mysql).
pub(crate) async fn file_readiness(path: &Path) -> ArtifactReadiness {
    match check_file_present(path).await {
        Ok(_) => ArtifactReadiness::NOT_SUPPORTED,
        Err(failed) => failed,
    }
}

/// Readiness of a SQLite copy: presence, then the in-process header check.
/// `None` is an id or path the plugin could not resolve.
pub(crate) async fn sqlite_readiness(path: Option<&Path>) -> ArtifactReadiness {
    let Some(path) = path else {
        return ArtifactReadiness::UNAVAILABLE;
    };
    let mut file = match check_file_present(path).await {
        Ok(file) => file,
        Err(failed) => return failed,
    };
    let mut header = [0u8; 16];
    match file.read_exact(&mut header).await {
        Ok(_) if &header == SQLITE_HEADER => ArtifactReadiness::STRUCTURAL,
        Ok(_) => ArtifactReadiness::CORRUPT,
        // A file shorter than the header cannot be a database.
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
            ArtifactReadiness::CORRUPT
        }
        Err(_) => ArtifactReadiness::UNAVAILABLE,
    }
}

/// Readiness of a custom-format dump: presence, then `pg_restore --list`.
///
/// A missing or slow tool is `Ready` / `PresenceOnly`, never `Invalid`; only a
/// tool that ran and failed reports corruption.
pub(crate) async fn pg_dump_readiness(pg_restore_bin: &str, path: &Path) -> ArtifactReadiness {
    if let Err(failed) = check_file_present(path).await {
        return failed;
    }
    let mut command = Command::new(pg_restore_bin);
    command
        .arg("--list")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        // The timeout drops the future; the tool must not outlive it.
        .kill_on_drop(true);
    match tokio::time::timeout(STRUCTURAL_TIMEOUT, command.status()).await {
        Err(_) => ArtifactReadiness::present(StructuralCheck::TimedOut),
        Ok(Err(_)) => ArtifactReadiness::present(StructuralCheck::ToolUnavailable),
        Ok(Ok(status)) if status.success() => ArtifactReadiness::STRUCTURAL,
        Ok(Ok(_)) => ArtifactReadiness::CORRUPT,
    }
}
