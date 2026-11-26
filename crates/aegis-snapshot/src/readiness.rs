use std::path::Path;
use std::time::Duration;

use aegis_types::{SnapshotAttemptReadiness, StructuralCheck, ValidationLevel};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

pub(crate) const ATTEMPT_TIMEOUT: Duration = Duration::from_millis(100);

pub(crate) const STRUCTURAL_TIMEOUT: Duration = Duration::from_millis(60);

const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtifactReadiness {
    pub readiness: SnapshotAttemptReadiness,
    pub validation_level: ValidationLevel,
    pub structural_check: StructuralCheck,
}

impl ArtifactReadiness {
    pub const UNAVAILABLE: Self = Self {
        readiness: SnapshotAttemptReadiness::Unavailable,
        validation_level: ValidationLevel::PresenceOnly,
        structural_check: StructuralCheck::NotRun,
    };

    pub const EMPTY: Self = Self {
        readiness: SnapshotAttemptReadiness::Invalid,
        validation_level: ValidationLevel::PresenceOnly,
        structural_check: StructuralCheck::NotRun,
    };

    pub const PRESENT_UNCHECKED: Self = Self::present(StructuralCheck::NotSupported);

    const fn present(check: StructuralCheck) -> Self {
        Self {
            readiness: SnapshotAttemptReadiness::Ready,
            validation_level: ValidationLevel::PresenceOnly,
            structural_check: check,
        }
    }

    const STRUCTURAL: Self = Self {
        readiness: SnapshotAttemptReadiness::Ready,
        validation_level: ValidationLevel::Structural,
        structural_check: StructuralCheck::Passed,
    };

    const CORRUPT: Self = Self {
        readiness: SnapshotAttemptReadiness::Invalid,
        validation_level: ValidationLevel::PresenceOnly,
        structural_check: StructuralCheck::Passed,
    };
}

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

pub(crate) async fn file_readiness(path: &Path) -> ArtifactReadiness {
    match check_file_present(path).await {
        Ok(_) => ArtifactReadiness::PRESENT_UNCHECKED,
        Err(failed) => failed,
    }
}

pub(crate) async fn sqlite_readiness(path: Option<&Path>) -> ArtifactReadiness {
    let Some(path) = path else {
        return ArtifactReadiness::UNAVAILABLE;
    };
    let file = match check_file_present(path).await {
        Ok(file) => file,
        Err(failed) => return failed,
    };
    let mut header = Vec::with_capacity(SQLITE_HEADER.len());
    match file.take(16).read_to_end(&mut header).await {
        Ok(_) if header == SQLITE_HEADER => ArtifactReadiness::STRUCTURAL,
        Ok(_) => ArtifactReadiness::CORRUPT,
        Err(_) => ArtifactReadiness::UNAVAILABLE,
    }
}

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
        .kill_on_drop(true);
    match tokio::time::timeout(STRUCTURAL_TIMEOUT, command.status()).await {
        Err(_) => ArtifactReadiness::present(StructuralCheck::TimedOut),
        Ok(Err(_)) => ArtifactReadiness::present(StructuralCheck::ToolUnavailable),
        Ok(Ok(status)) if status.success() => ArtifactReadiness::STRUCTURAL,
        Ok(Ok(_)) => ArtifactReadiness::CORRUPT,
    }
}
