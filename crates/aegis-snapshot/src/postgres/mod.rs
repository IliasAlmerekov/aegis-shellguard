//! PostgreSQL snapshot provider.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use tokio::process::Command;

use crate::busy;
use crate::containment::contain_artifact;
use crate::database_target::{DatabaseTarget, RollbackTarget};
use crate::error::SnapshotError;
use crate::liveness::local_file_liveness;
use crate::readiness::pg_dump_readiness;
use crate::secure_fs::{create_store_dir, harden_existing_artifact};
use crate::snapshot_id;
use crate::{ArtifactReadiness, Liveness, SnapshotPlugin};

type Result<T> = std::result::Result<T, SnapshotError>;

const BUSY_RETRY_ATTEMPTS: usize = 12;

/// Snapshot plugin for PostgreSQL databases.
pub struct PostgresPlugin {
    target: DatabaseTarget,
    pg_dump_bin: String,
    pg_restore_bin: String,
}

impl PostgresPlugin {
    /// Create a new PostgreSQL snapshot plugin.
    pub fn new(
        database: String,
        host: String,
        port: u16,
        user: String,
        snapshots_dir: PathBuf,
    ) -> Self {
        Self {
            target: DatabaseTarget {
                database,
                host,
                port,
                user,
                snapshots_dir,
                prefix: "pg",
                extension: "dump",
                plugin: "postgres",
            },
            pg_dump_bin: "pg_dump".to_string(),
            pg_restore_bin: "pg_restore".to_string(),
        }
    }

    fn build_common_args(&self) -> Vec<String> {
        Self::build_common_args_for_target(&self.target.host, self.target.port, &self.target.user)
    }

    fn build_common_args_for_target(host: &str, port: u16, user: &str) -> Vec<String> {
        let mut args = vec![
            "-h".to_string(),
            host.to_string(),
            "-p".to_string(),
            port.to_string(),
        ];

        if !user.is_empty() {
            args.push("-U".to_string());
            args.push(user.to_string());
        }

        args
    }

    fn resolve_snapshot_id(&self, id: &str) -> Result<RollbackTarget> {
        match self.target.parse_snapshot_id(id)? {
            Some(target) => Ok(target),
            None => self.parse_legacy_snapshot_id(id),
        }
    }

    fn parse_legacy_snapshot_id(&self, snapshot_id: &str) -> Result<RollbackTarget> {
        let (database_encoded, dump_str) = snapshot_id
            .split_once(snapshot_id::LEGACY_SEP)
            .ok_or_else(|| SnapshotError::MalformedSnapshotId {
                id: snapshot_id.to_string(),
            })?;
        let _database = snapshot_id::decode_string(database_encoded, "database")?;
        Ok(RollbackTarget {
            host: self.target.host.clone(),
            port: self.target.port,
            user: self.target.user.clone(),
            dump_path: PathBuf::from(dump_str),
        })
    }
}

#[async_trait]
impl SnapshotPlugin for PostgresPlugin {
    fn name(&self) -> &'static str {
        "postgres"
    }

    async fn is_applicable(&self, _cwd: &Path) -> bool {
        if self.target.database.is_empty() {
            return false;
        }

        tokio::process::Command::new("which")
            .arg(&self.pg_dump_bin)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .map(|status| status.success())
            .unwrap_or(false)
    }

    async fn snapshot(&self, _cwd: &Path, _cmd: &str) -> Result<String> {
        create_store_dir("postgres", &self.target.snapshots_dir)?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let dump_path = self.target.reserve_dump_path(timestamp)?;

        let mut args = self.build_common_args();
        args.extend([
            "-Fc".to_string(),
            "-f".to_string(),
            dump_path.display().to_string(),
            self.target.database.clone(),
        ]);

        let mut command = Command::new(&self.pg_dump_bin);
        command.args(&args);
        let output = match busy::retry(
            || command.output(),
            BUSY_RETRY_ATTEMPTS,
            &self.pg_dump_bin,
            "postgres",
            "failed to run pg_dump",
        )
        .await
        {
            Ok(output) => output,
            Err(error) => {
                let _ = std::fs::remove_file(&dump_path);
                return Err(error);
            }
        };

        if !output.status.success() {
            let _ = std::fs::remove_file(&dump_path);
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(SnapshotError::CommandFailed {
                command: "pg_dump".to_string(),
                detail: stderr,
            });
        }

        if let Err(error) = harden_existing_artifact("postgres", &dump_path) {
            let _ = std::fs::remove_file(&dump_path);
            return Err(error);
        }

        let dump_path = dump_path.canonicalize()?;
        let snapshot_id = self.target.build_snapshot_id(&dump_path);
        tracing::info!(%snapshot_id, "postgres snapshot created");
        Ok(snapshot_id)
    }

    async fn rollback(&self, snapshot_id: &str) -> Result<()> {
        let mut target = self.resolve_snapshot_id(snapshot_id)?;
        target.dump_path =
            contain_artifact("postgres", &self.target.snapshots_dir, &target.dump_path)?;
        if !target.dump_path.exists() {
            return Err(SnapshotError::RollbackDumpNotFound {
                path: target.dump_path.to_string_lossy().to_string(),
            });
        }

        let mut args = Self::build_common_args_for_target(&target.host, target.port, &target.user);
        args.extend([
            "--clean".to_string(),
            "--if-exists".to_string(),
            "--create".to_string(),
            "-d".to_string(),
            "postgres".to_string(),
            target.dump_path.to_string_lossy().to_string(),
        ]);

        let mut command = Command::new(&self.pg_restore_bin);
        command.args(&args);
        let output = busy::retry(
            || command.output(),
            BUSY_RETRY_ATTEMPTS,
            &self.pg_restore_bin,
            "postgres",
            "failed to run pg_restore",
        )
        .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(SnapshotError::CommandFailed {
                command: "pg_restore".to_string(),
                detail: stderr,
            });
        }

        tracing::info!(snapshot_id = snapshot_id, "postgres snapshot rolled back");
        Ok(())
    }

    async fn liveness(&self, snapshot_id: &str) -> Result<Liveness> {
        let target = self.resolve_snapshot_id(snapshot_id)?;
        let dump_path =
            contain_artifact("postgres", &self.target.snapshots_dir, &target.dump_path)?;
        Ok(local_file_liveness(&dump_path).await)
    }

    async fn readiness(&self, snapshot_id: &str) -> ArtifactReadiness {
        let Ok(target) = self.resolve_snapshot_id(snapshot_id) else {
            return ArtifactReadiness::UNAVAILABLE;
        };
        let Ok(dump_path) =
            contain_artifact("postgres", &self.target.snapshots_dir, &target.dump_path)
        else {
            return ArtifactReadiness::UNAVAILABLE;
        };
        pg_dump_readiness(&self.pg_restore_bin, &dump_path).await
    }

    async fn delete(&self, snapshot_id: &str) -> Result<()> {
        let mut target = self.resolve_snapshot_id(snapshot_id)?;
        target.dump_path =
            contain_artifact("postgres", &self.target.snapshots_dir, &target.dump_path)?;
        match std::fs::remove_file(&target.dump_path) {
            Ok(()) => {
                tracing::info!(path = %target.dump_path.display(), "postgres dump deleted");
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                tracing::info!(path = %target.dump_path.display(), "postgres dump already removed");
                Ok(())
            }
            Err(error) => Err(SnapshotError::DeleteFailed {
                plugin: "postgres".to_string(),
                snapshot_id: snapshot_id.to_string(),
                detail: error.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests;
