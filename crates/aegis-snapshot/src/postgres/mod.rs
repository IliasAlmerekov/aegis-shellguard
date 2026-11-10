//! PostgreSQL snapshot provider.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use tokio::process::Command;

use crate::containment::contain_artifact;
use crate::error::SnapshotError;
use crate::liveness::local_file_liveness;
use crate::readiness::pg_dump_readiness;
use crate::secure_fs::{create_artifact_file, create_store_dir, harden_existing_artifact};
use crate::snapshot_id;
use crate::{ArtifactReadiness, Liveness, SnapshotPlugin};

type Result<T> = std::result::Result<T, SnapshotError>;

const EXECUTABLE_BUSY_ERRNO: i32 = 26;
const BUSY_RETRY_ATTEMPTS: usize = 12;
const BUSY_RETRY_DELAY_MS: u64 = 25;

struct PostgresRollbackTarget {
    host: String,
    port: u16,
    user: String,
    dump_path: PathBuf,
}

/// Snapshot plugin for PostgreSQL databases.
pub struct PostgresPlugin {
    database: String,
    host: String,
    port: u16,
    user: String,
    snapshots_dir: PathBuf,
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
            database,
            host,
            port,
            user,
            snapshots_dir,
            pg_dump_bin: "pg_dump".to_string(),
            pg_restore_bin: "pg_restore".to_string(),
        }
    }

    fn build_common_args(&self) -> Vec<String> {
        Self::build_common_args_for_target(&self.host, self.port, &self.user)
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

    fn dump_path_candidate(&self, timestamp: u64, suffix: Option<usize>) -> PathBuf {
        let base_name = format!("pg-{}-{timestamp}", self.sanitized_database_label());
        let file_name = match suffix {
            Some(suffix) => format!("{base_name}-{suffix}.dump"),
            None => format!("{base_name}.dump"),
        };

        self.snapshots_dir.join(file_name)
    }

    fn reserve_dump_path(&self, timestamp: u64) -> Result<PathBuf> {
        let mut suffix = None;

        loop {
            let dump_path = contain_artifact(
                "postgres",
                &self.snapshots_dir,
                &self.dump_path_candidate(timestamp, suffix),
            )?;
            match create_artifact_file("postgres", &dump_path) {
                Ok(file) => {
                    drop(file);
                    return Ok(dump_path);
                }
                Err(SnapshotError::Io(err)) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                    suffix = Some(suffix.map_or(1, |current| current + 1));
                }
                Err(err) => return Err(err),
            }
        }
    }

    async fn output_with_busy_retry(
        command: &mut Command,
        context: &str,
    ) -> Result<std::process::Output> {
        let mut attempt = 0usize;
        loop {
            match command.output().await {
                Ok(output) => return Ok(output),
                Err(error) if Self::is_executable_busy(&error) && attempt < BUSY_RETRY_ATTEMPTS => {
                    attempt += 1;
                    tracing::warn!(
                        context,
                        attempt,
                        "postgres binary busy during command launch, retrying"
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(BUSY_RETRY_DELAY_MS)).await;
                }
                Err(error) => {
                    return Err(SnapshotError::Snapshot(format!("{context}: {error}")));
                }
            }
        }
    }

    fn sanitized_database_label(&self) -> String {
        self.database
            .chars()
            .map(|ch| match ch {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => ch,
                _ => '_',
            })
            .collect()
    }

    fn is_executable_busy(error: &std::io::Error) -> bool {
        error.raw_os_error() == Some(EXECUTABLE_BUSY_ERRNO)
    }

    fn build_snapshot_id(&self, dump_path: &Path) -> String {
        snapshot_id::join_v3(&[
            &snapshot_id::encode_bytes(self.database.as_bytes()),
            &snapshot_id::encode_bytes(self.host.as_bytes()),
            &self.port.to_string(),
            &snapshot_id::encode_bytes(self.user.as_bytes()),
            &snapshot_id::encode_path(dump_path),
        ])
    }

    fn parse_snapshot_id(&self, id: &str) -> Result<PostgresRollbackTarget> {
        let parts = if snapshot_id::is_v3(id) {
            snapshot_id::split_v3(id, 5)?
        } else if snapshot_id::is_v2(id) {
            snapshot_id::split_v2(id, 5)?
        } else {
            return self.parse_legacy_snapshot_id(id);
        };

        let _database = snapshot_id::decode_string(parts[0], "database")?;
        let host = snapshot_id::decode_string(parts[1], "host")?;
        let port = parts[2].parse::<u16>().map_err(|_| {
            SnapshotError::Snapshot(format!(
                "malformed snapshot_id: invalid port {:?}",
                parts[2]
            ))
        })?;
        let user = snapshot_id::decode_string(parts[3], "user")?;
        let dump_path = snapshot_id::decode_path(parts[4], "dump path")?;

        Ok(PostgresRollbackTarget {
            host,
            port,
            user,
            dump_path,
        })
    }

    fn parse_legacy_snapshot_id(&self, snapshot_id: &str) -> Result<PostgresRollbackTarget> {
        let (database_encoded, dump_str) = snapshot_id
            .split_once(snapshot_id::LEGACY_SEP)
            .ok_or_else(|| {
                SnapshotError::Snapshot(format!("malformed snapshot_id: {snapshot_id:?}"))
            })?;
        let _database = snapshot_id::decode_string(database_encoded, "database")?;
        Ok(PostgresRollbackTarget {
            host: self.host.clone(),
            port: self.port,
            user: self.user.clone(),
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
        if self.database.is_empty() {
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
        create_store_dir("postgres", &self.snapshots_dir)?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let dump_path = self.reserve_dump_path(timestamp)?;

        let mut args = self.build_common_args();
        args.extend([
            "-Fc".to_string(),
            "-f".to_string(),
            dump_path.display().to_string(),
            self.database.clone(),
        ]);

        let mut command = Command::new(&self.pg_dump_bin);
        command.args(&args);
        let output = match Self::output_with_busy_retry(&mut command, "failed to run pg_dump").await
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
            return Err(SnapshotError::Snapshot(format!("pg_dump failed: {stderr}")));
        }

        if let Err(error) = harden_existing_artifact("postgres", &dump_path) {
            let _ = std::fs::remove_file(&dump_path);
            return Err(error);
        }

        let dump_path = dump_path.canonicalize()?;
        let snapshot_id = self.build_snapshot_id(&dump_path);
        tracing::info!(%snapshot_id, "postgres snapshot created");
        Ok(snapshot_id)
    }

    async fn rollback(&self, snapshot_id: &str) -> Result<()> {
        let mut target = self.parse_snapshot_id(snapshot_id)?;
        target.dump_path = contain_artifact("postgres", &self.snapshots_dir, &target.dump_path)?;
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
        let output = Self::output_with_busy_retry(&mut command, "failed to run pg_restore").await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(SnapshotError::Snapshot(format!(
                "pg_restore failed: {stderr}"
            )));
        }

        tracing::info!(snapshot_id = snapshot_id, "postgres snapshot rolled back");
        Ok(())
    }

    async fn liveness(&self, snapshot_id: &str) -> Result<Liveness> {
        let target = self.parse_snapshot_id(snapshot_id)?;
        let dump_path = contain_artifact("postgres", &self.snapshots_dir, &target.dump_path)?;
        Ok(local_file_liveness(&dump_path).await)
    }

    async fn readiness(&self, snapshot_id: &str) -> ArtifactReadiness {
        let Ok(target) = self.parse_snapshot_id(snapshot_id) else {
            return ArtifactReadiness::UNAVAILABLE;
        };
        let Ok(dump_path) = contain_artifact("postgres", &self.snapshots_dir, &target.dump_path)
        else {
            return ArtifactReadiness::UNAVAILABLE;
        };
        pg_dump_readiness(&self.pg_restore_bin, &dump_path).await
    }

    async fn delete(&self, snapshot_id: &str) -> Result<()> {
        let mut target = self.parse_snapshot_id(snapshot_id)?;
        target.dump_path = contain_artifact("postgres", &self.snapshots_dir, &target.dump_path)?;
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
                source: error.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests;
