//! MySQL snapshot provider.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::process::{ChildStderr, Command};
use tokio::task::JoinHandle;

use crate::busy;
use crate::containment::contain_artifact;
use crate::database_target::{DatabaseTarget, RollbackTarget};
use crate::error::SnapshotError;
use crate::liveness::local_file_liveness;
use crate::readiness::file_readiness;
use crate::secure_fs::{create_store_dir, harden_existing_artifact};
use crate::snapshot_id;
use crate::{ArtifactReadiness, Liveness, SnapshotPlugin};

type Result<T> = std::result::Result<T, SnapshotError>;

const BUSY_RETRY_ATTEMPTS: usize = 12;

/// Snapshot plugin for MySQL databases.
pub struct MysqlPlugin {
    target: DatabaseTarget,
    mysqldump_bin: String,
    mysql_bin: String,
}

impl MysqlPlugin {
    /// Create a new MySQL snapshot plugin.
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
                prefix: "mysql",
                extension: "sql",
                plugin: "mysql",
            },
            mysqldump_bin: "mysqldump".to_string(),
            mysql_bin: "mysql".to_string(),
        }
    }

    fn build_common_args(&self) -> Vec<String> {
        Self::build_common_args_for_target(&self.target.host, self.target.port, &self.target.user)
    }

    fn build_common_args_for_target(host: &str, port: u16, user: &str) -> Vec<String> {
        let mut args = vec![format!("--host={host}"), format!("--port={port}")];

        if !user.is_empty() {
            args.push(format!("--user={user}"));
        }

        args
    }

    fn validate_dump_file_name(dump_file_name: &str) -> Result<()> {
        let path = Path::new(dump_file_name);
        let mut components = path.components();
        let is_plain_file_name = matches!(
            (components.next(), components.next()),
            (Some(std::path::Component::Normal(name)), None)
                if name.to_str() == Some(dump_file_name)
        );
        if !is_plain_file_name {
            return Err(SnapshotError::InvalidSnapshotIdComponent {
                component: "dump reference".to_string(),
                value: dump_file_name.to_string(),
            });
        }

        Ok(())
    }

    fn resolve_snapshot_id(&self, id: &str) -> Result<RollbackTarget> {
        match self.target.parse_snapshot_id(id)? {
            Some(target) => Ok(target),
            None => self.parse_legacy_snapshot_id(id),
        }
    }

    fn parse_legacy_snapshot_id(&self, snapshot_id: &str) -> Result<RollbackTarget> {
        let (database_encoded, dump_ref_encoded) = snapshot_id
            .split_once(snapshot_id::LEGACY_SEP)
            .ok_or_else(|| SnapshotError::MalformedSnapshotId {
                id: snapshot_id.to_string(),
            })?;
        let _database = snapshot_id::decode_string(database_encoded, "database")?;
        let dump_file_name = snapshot_id::decode_string(dump_ref_encoded, "dump reference")?;
        Self::validate_dump_file_name(&dump_file_name)?;

        Ok(RollbackTarget {
            host: self.target.host.clone(),
            port: self.target.port,
            user: self.target.user.clone(),
            dump_path: self.target.snapshots_dir.join(dump_file_name),
        })
    }

    async fn kill_and_reap_child(child: &mut tokio::process::Child) {
        // Best-effort child cleanup on an error path; the caller already has a failure to return.
        let _ = child.kill().await;
        let _ = child.wait().await;
    }

    fn spawn_stderr_drain(stderr: ChildStderr) -> JoinHandle<std::io::Result<Vec<u8>>> {
        tokio::spawn(async move {
            let mut stderr = stderr;
            let mut bytes = Vec::new();
            stderr.read_to_end(&mut bytes).await?;
            Ok(bytes)
        })
    }

    async fn collect_stderr(
        stderr_task: JoinHandle<std::io::Result<Vec<u8>>>,
        command_name: &str,
    ) -> Result<Vec<u8>> {
        stderr_task
            .await
            .map_err(|err| SnapshotError::OperationFailed {
                action: format!("join {command_name} stderr task"),
                detail: err.to_string(),
            })?
            .map_err(|err| SnapshotError::OperationFailed {
                action: format!("read {command_name} stderr"),
                detail: err.to_string(),
            })
    }
}

#[async_trait]
impl SnapshotPlugin for MysqlPlugin {
    fn name(&self) -> &'static str {
        "mysql"
    }

    async fn is_applicable(&self, _cwd: &Path) -> bool {
        if self.target.database.is_empty() {
            return false;
        }

        crate::binary_lookup::binary_available(&self.mysqldump_bin).await
    }

    async fn snapshot(&self, _cwd: &Path, _cmd: &str) -> Result<String> {
        create_store_dir("mysql", &self.target.snapshots_dir)?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let dump_path = self.target.reserve_dump_path(timestamp)?;

        let mut args = self.build_common_args();
        args.extend([
            "--databases".to_string(),
            self.target.database.clone(),
            "--add-drop-database".to_string(),
        ]);

        let mut command = Command::new(&self.mysqldump_bin);
        command
            .args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let mut child = match busy::retry(
            || std::future::ready(command.spawn()),
            BUSY_RETRY_ATTEMPTS,
            &self.mysqldump_bin,
            "mysql",
            "failed to run mysqldump",
        )
        .await
        {
            Ok(child) => child,
            Err(err) => {
                // Best-effort cleanup of the reserved dump file; preserve the primary error.
                let _ = std::fs::remove_file(&dump_path);
                return Err(err);
            }
        };

        let mut stdout = child.stdout.take().ok_or_else(|| {
            // Best-effort cleanup of the reserved dump file; preserve the primary error.
            let _ = std::fs::remove_file(&dump_path);
            SnapshotError::StreamUnavailable {
                action: "capture mysqldump stdout",
            }
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            // Best-effort cleanup of the reserved dump file; preserve the primary error.
            let _ = std::fs::remove_file(&dump_path);
            SnapshotError::StreamUnavailable {
                action: "capture mysqldump stderr",
            }
        })?;
        let stderr_task = Self::spawn_stderr_drain(stderr);
        let mut dump_file = tokio::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&dump_path)
            .await?;
        if let Err(err) = io::copy(&mut stdout, &mut dump_file).await {
            Self::kill_and_reap_child(&mut child).await;
            // Best-effort stderr drain and dump cleanup; preserve the primary copy error.
            let _ = Self::collect_stderr(stderr_task, "mysqldump").await;
            let _ = std::fs::remove_file(&dump_path);
            return Err(SnapshotError::OperationFailed {
                action: "stream mysqldump output".to_string(),
                detail: err.to_string(),
            });
        }
        drop(stdout);
        dump_file.flush().await?;
        drop(dump_file);

        let status = child
            .wait()
            .await
            .map_err(|e| SnapshotError::OperationFailed {
                action: "wait for mysqldump".to_string(),
                detail: e.to_string(),
            })?;
        let stderr = Self::collect_stderr(stderr_task, "mysqldump").await?;

        if !status.success() {
            // Best-effort cleanup of the reserved dump file; preserve the primary error.
            let _ = std::fs::remove_file(&dump_path);
            let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
            return Err(SnapshotError::CommandFailed {
                command: "mysqldump".to_string(),
                detail: stderr,
            });
        }

        if let Err(error) = harden_existing_artifact("mysql", &dump_path) {
            // Best-effort cleanup of the reserved dump file; preserve the primary error.
            let _ = std::fs::remove_file(&dump_path);
            return Err(error);
        }

        let dump_path = dump_path.canonicalize()?;
        let snapshot_id = self.target.build_snapshot_id(&dump_path);
        tracing::info!(%snapshot_id, "mysql snapshot created");
        Ok(snapshot_id)
    }

    async fn rollback(&self, snapshot_id: &str) -> Result<()> {
        let mut target = self.resolve_snapshot_id(snapshot_id)?;
        target.dump_path =
            contain_artifact("mysql", &self.target.snapshots_dir, &target.dump_path)?;
        if !target.dump_path.exists() {
            return Err(SnapshotError::RollbackDumpNotFound {
                path: target.dump_path.to_string_lossy().to_string(),
            });
        }

        let args = Self::build_common_args_for_target(&target.host, target.port, &target.user);
        let mut command = Command::new(&self.mysql_bin);
        command
            .args(&args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped());
        let mut child = busy::retry(
            || std::future::ready(command.spawn()),
            BUSY_RETRY_ATTEMPTS,
            &self.mysql_bin,
            "mysql",
            "failed to run mysql",
        )
        .await?;

        let mut dump_file = tokio::fs::File::open(&target.dump_path).await?;
        let stderr = child
            .stderr
            .take()
            .ok_or(SnapshotError::StreamUnavailable {
                action: "capture mysql stderr",
            })?;
        let stderr_task = Self::spawn_stderr_drain(stderr);
        let mut stdin = child.stdin.take().ok_or(SnapshotError::StreamUnavailable {
            action: "open mysql stdin",
        })?;
        if let Err(err) = io::copy(&mut dump_file, &mut stdin).await {
            drop(stdin);
            Self::kill_and_reap_child(&mut child).await;
            // Best-effort stderr drain; preserve the primary stdin copy error.
            let _ = Self::collect_stderr(stderr_task, "mysql").await;
            return Err(SnapshotError::OperationFailed {
                action: "write mysql stdin".to_string(),
                detail: err.to_string(),
            });
        }
        drop(stdin);

        let status = child
            .wait()
            .await
            .map_err(|e| SnapshotError::OperationFailed {
                action: "wait for mysql".to_string(),
                detail: e.to_string(),
            })?;
        let stderr = Self::collect_stderr(stderr_task, "mysql").await?;

        if !status.success() {
            let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
            return Err(SnapshotError::CommandFailed {
                command: "mysql".to_string(),
                detail: stderr,
            });
        }

        tracing::info!(snapshot_id = snapshot_id, "mysql snapshot rolled back");
        Ok(())
    }

    async fn liveness(&self, snapshot_id: &str) -> Result<Liveness> {
        let target = self.resolve_snapshot_id(snapshot_id)?;
        let dump_path = contain_artifact("mysql", &self.target.snapshots_dir, &target.dump_path)?;
        Ok(local_file_liveness(&dump_path).await)
    }

    async fn readiness(&self, snapshot_id: &str) -> ArtifactReadiness {
        let Ok(target) = self.resolve_snapshot_id(snapshot_id) else {
            return ArtifactReadiness::UNAVAILABLE;
        };
        let Ok(dump_path) =
            contain_artifact("mysql", &self.target.snapshots_dir, &target.dump_path)
        else {
            return ArtifactReadiness::UNAVAILABLE;
        };
        file_readiness(&dump_path).await
    }

    async fn delete(&self, snapshot_id: &str) -> Result<()> {
        let mut target = self.resolve_snapshot_id(snapshot_id)?;
        target.dump_path =
            contain_artifact("mysql", &self.target.snapshots_dir, &target.dump_path)?;
        match std::fs::remove_file(&target.dump_path) {
            Ok(()) => {
                tracing::info!(path = %target.dump_path.display(), "mysql dump deleted");
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                tracing::info!(path = %target.dump_path.display(), "mysql dump already removed");
                Ok(())
            }
            Err(error) => Err(SnapshotError::DeleteFailed {
                plugin: "mysql".to_string(),
                snapshot_id: snapshot_id.to_string(),
                detail: error.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests;
