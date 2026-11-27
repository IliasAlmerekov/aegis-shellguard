use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::process::Command;

use aegis_config::SupabaseSnapshotConfig;

use crate::error::SnapshotError;
use crate::secure_fs::{
    create_artifact_dir, create_artifact_file, create_store_dir, harden_existing_artifact,
};
use crate::snapshot_id;
use crate::{Liveness, SnapshotPlugin};

type Result<T> = std::result::Result<T, SnapshotError>;

const LEGACY_SNAPSHOT_ID_VERSION: &str = "supabase-v1";
const MANIFEST_FILE_NAME: &str = "manifest.json";

#[cfg(test)]
thread_local! {
    static INJECT_MANIFEST_WRITE_FAILURE_FOR_TESTS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SupabaseOverallStatus {
    Complete,
    Partial,
    Degraded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SupabaseCapabilities {
    db: bool,
    storage: bool,
    functions: bool,
    project_config: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SupabaseTarget {
    project_ref: String,
    db: Option<SupabaseTargetDb>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SupabaseTargetDb {
    database: String,
    host: String,
    port: u16,
    user: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SupabaseDbArtifact {
    present: bool,
    complete: bool,
    path: Option<String>,
    format: Option<String>,
    checksum_sha256: Option<String>,
    size_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SupabaseArtifacts {
    db: SupabaseDbArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct SupabaseRollback {
    db_supported: bool,
    allowed: bool,
    config_target_match_required: bool,
    reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SupabaseStrictEligibility {
    db_supported: bool,
    allowed: bool,
    overall_status: SupabaseOverallStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SupabaseManifest {
    manifest_version: u32,
    provider: String,
    created_at: String,
    capabilities: SupabaseCapabilities,
    target: SupabaseTarget,
    artifacts: SupabaseArtifacts,
    rollback: SupabaseRollback,
    partial: bool,
    degraded: bool,
    warnings: Vec<String>,
    errors: Vec<String>,
    overall_status: SupabaseOverallStatus,
}

fn sha256_hex(path: &Path) -> Result<String> {
    let file = fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
    }

    let digest = hasher.finalize();
    let mut checksum = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(checksum, "{byte:02x}");
    }
    Ok(checksum)
}

fn sync_parent_directory(path: &Path) -> Result<()> {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let dir = fs::File::open(path)?;
        dir.sync_all()?;
        Ok(())
    }

    #[cfg(any(not(unix), target_os = "macos"))]
    {
        let _ = path;
        Ok(())
    }
}

mod runtime;

pub use runtime::SupabasePlugin;
