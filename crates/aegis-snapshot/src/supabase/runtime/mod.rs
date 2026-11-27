use super::*;
use crate::ArtifactReadiness;
use crate::containment::contain_artifact;
use crate::liveness::local_file_liveness;
use crate::readiness::pg_dump_readiness;

use manifest_io::write_manifest_atomically;
use manifest_state::phase1_complete;

mod manifest_io;
mod manifest_state;
mod rollback;

#[cfg(test)]
mod tests;

fn eligible_dump_path(manifest: &SupabaseManifest, manifest_path: &Path) -> Result<PathBuf> {
    manifest.validate_schema_v1()?;

    let strict = manifest.recompute_strict_eligibility()?;
    if !strict.allowed {
        return Err(SnapshotError::RollbackDenied {
            status: format!("{:?}", strict.overall_status),
        });
    }
    manifest.ensure_summary_matches_recomputed(&strict)?;
    manifest.resolve_db_artifact_path(manifest_path)
}

pub struct SupabasePlugin {
    project_ref: String,
    db: SupabaseTargetDb,
    require_target_match: bool,
    snapshots_dir: PathBuf,
    pg_dump_bin: String,
    pg_restore_bin: String,
    #[cfg(test)]
    inject_manifest_write_failure_for_tests: bool,
}

impl SupabasePlugin {
    pub fn new(config: SupabaseSnapshotConfig, snapshots_dir: PathBuf) -> Self {
        Self {
            project_ref: config.project_ref,
            db: SupabaseTargetDb {
                database: config.db.database,
                host: config.db.host,
                port: config.db.port,
                user: config.db.user,
            },
            require_target_match: config.require_config_target_match_on_rollback,
            snapshots_dir,
            pg_dump_bin: "pg_dump".to_string(),
            pg_restore_bin: "pg_restore".to_string(),
            #[cfg(test)]
            inject_manifest_write_failure_for_tests: false,
        }
    }

    fn has_database_target(&self) -> bool {
        !self.db.database.trim().is_empty()
    }

    fn ensure_rollback_target_allowed(&self, manifest: &SupabaseManifest) -> Result<()> {
        if self.require_target_match {
            manifest.ensure_target_matches(&self.db)?;
        }
        Ok(())
    }

    fn target(&self) -> SupabaseTarget {
        SupabaseTarget {
            project_ref: self.project_ref.clone(),
            db: Some(self.db.clone()),
        }
    }

    fn validate_manifest_path(path: &Path, label: &str) -> Result<()> {
        if !path.is_absolute()
            || path.file_name() != Some(MANIFEST_FILE_NAME.as_ref())
            || path.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::CurDir | std::path::Component::ParentDir
                )
            })
        {
            return Err(SnapshotError::InvalidSnapshotIdPath {
                label: label.to_string(),
                path: path.to_path_buf(),
            });
        }

        Ok(())
    }

    fn build_snapshot_id(manifest_path: &Path) -> String {
        snapshot_id::join_v3(&[&snapshot_id::encode_path(manifest_path)])
    }

    fn parse_snapshot_id(id: &str) -> Result<PathBuf> {
        let encoded_path = if snapshot_id::is_v3(id) {
            snapshot_id::split_v3(id, 1)?[0]
        } else {
            let (version, encoded_path) = id
                .split_once(snapshot_id::LEGACY_SEP)
                .ok_or_else(|| SnapshotError::MalformedSnapshotId { id: id.to_string() })?;
            if version != LEGACY_SNAPSHOT_ID_VERSION {
                return Err(SnapshotError::UnsupportedSnapshotIdVersion {
                    version: version.to_string(),
                });
            }
            encoded_path
        };

        let label = "manifest path";
        let path = snapshot_id::decode_path(encoded_path, label)?;
        Self::validate_manifest_path(&path, label)?;
        Ok(path)
    }

    async fn validate_preflight(&self) -> Result<()> {
        if !self.has_database_target() {
            return Err(SnapshotError::Required {
                subject: "supabase_snapshot.db.database",
            });
        }
        if !crate::binary_lookup::binary_available(&self.pg_dump_bin).await {
            return Err(SnapshotError::RequiredWhen {
                subject: "pg_dump",
                condition: "for supabase snapshots",
            });
        }
        if !crate::binary_lookup::binary_available(&self.pg_restore_bin).await {
            return Err(SnapshotError::RequiredWhen {
                subject: "pg_restore",
                condition: "for strict supabase rollback",
            });
        }
        Ok(())
    }

    fn create_bundle_dir(&self) -> Result<PathBuf> {
        create_store_dir("supabase", &self.snapshots_dir)?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);

        let mut suffix = 0usize;
        loop {
            let dir_name = if suffix == 0 {
                format!("supabase-{timestamp}")
            } else {
                format!("supabase-{timestamp}-{suffix}")
            };
            let bundle_dir = contain_artifact(
                "supabase",
                &self.snapshots_dir,
                &self.snapshots_dir.join(dir_name),
            )?;

            match create_artifact_dir("supabase", &bundle_dir) {
                Ok(()) => return Ok(bundle_dir),
                Err(SnapshotError::Io(error))
                    if error.kind() == std::io::ErrorKind::AlreadyExists =>
                {
                    suffix += 1;
                }
                Err(error) => return Err(error),
            }
        }
    }

    fn cleanup_failed_bundle(&self, bundle_dir: &Path, dump_path: Option<&Path>) -> Result<()> {
        let mut failures = Vec::new();

        if let Some(path) = dump_path {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    failures.push(format!("failed to remove dump {}: {error}", path.display()))
                }
            }
        }

        match fs::remove_dir_all(bundle_dir) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => failures.push(format!(
                "failed to remove bundle {}: {error}",
                bundle_dir.display()
            )),
        }

        if failures.is_empty() {
            Ok(())
        } else {
            Err(SnapshotError::CleanupIncomplete { failures })
        }
    }

    fn fail_closed_after_cleanup(
        &self,
        original_error: SnapshotError,
        bundle_dir: &Path,
        dump_path: Option<&Path>,
    ) -> SnapshotError {
        match self.cleanup_failed_bundle(bundle_dir, dump_path) {
            Ok(()) => original_error,
            Err(cleanup_error) => SnapshotError::CleanupFailed {
                original: Box::new(original_error),
                cleanup: Box::new(cleanup_error),
            },
        }
    }

    async fn run_pg_dump(&self, dump_path: &Path) -> Result<()> {
        let artifacts_dir = dump_path.parent().ok_or(SnapshotError::Rejected {
            reason: "supabase dump path requires an artifact directory",
        })?;
        let dump_path = contain_artifact("supabase", artifacts_dir, dump_path)?;
        let file = create_artifact_file("supabase", &dump_path)?;
        drop(file);
        let output = Command::new(&self.pg_dump_bin)
            .arg("-Fc")
            .arg("-h")
            .arg(&self.db.host)
            .arg("-p")
            .arg(self.db.port.to_string())
            .arg("-U")
            .arg(&self.db.user)
            .arg("-f")
            .arg(&dump_path)
            .arg(&self.db.database)
            .output()
            .await
            .map_err(|error| SnapshotError::CommandLaunchFailed {
                context: "failed to run pg_dump".to_string(),
                detail: error.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(SnapshotError::CommandFailed {
                command: "pg_dump".to_string(),
                detail: stderr,
            });
        }

        harden_existing_artifact("supabase", &dump_path)?;

        Ok(())
    }

    async fn run_pg_restore(&self, target: &SupabaseTargetDb, dump_path: &Path) -> Result<()> {
        let output = Command::new(&self.pg_restore_bin)
            .arg("--clean")
            .arg("--if-exists")
            .arg("--create")
            .arg("-h")
            .arg(&target.host)
            .arg("-p")
            .arg(target.port.to_string())
            .arg("-U")
            .arg(&target.user)
            .arg("-d")
            .arg("postgres")
            .arg(dump_path)
            .output()
            .await
            .map_err(|error| SnapshotError::CommandLaunchFailed {
                context: "failed to run pg_restore".to_string(),
                detail: error.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(SnapshotError::CommandFailed {
                command: "pg_restore".to_string(),
                detail: stderr,
            });
        }

        Ok(())
    }
}

#[async_trait]
impl SnapshotPlugin for SupabasePlugin {
    fn name(&self) -> &'static str {
        "supabase"
    }

    async fn is_applicable(&self, _cwd: &Path) -> bool {
        self.has_database_target()
            && crate::binary_lookup::binary_available(&self.pg_dump_bin).await
            && crate::binary_lookup::binary_available(&self.pg_restore_bin).await
    }

    async fn snapshot(&self, _cwd: &Path, _cmd: &str) -> Result<String> {
        self.validate_preflight().await?;

        let bundle_dir = self.create_bundle_dir()?;
        let artifacts_dir =
            contain_artifact("supabase", &bundle_dir, &bundle_dir.join("artifacts"))?;
        if let Err(error) = create_store_dir("supabase", &artifacts_dir) {
            return Err(self.fail_closed_after_cleanup(error, &bundle_dir, None));
        }

        let dump_path = artifacts_dir.join("db.dump");
        if let Err(error) = self.run_pg_dump(&dump_path).await {
            return Err(self.fail_closed_after_cleanup(error, &bundle_dir, Some(&dump_path)));
        }

        let checksum_sha256 = match sha256_hex(&dump_path) {
            Ok(checksum_sha256) => checksum_sha256,
            Err(error) => {
                return Err(self.fail_closed_after_cleanup(error, &bundle_dir, Some(&dump_path)));
            }
        };

        let size_bytes = match fs::metadata(&dump_path) {
            Ok(metadata) => metadata.len(),
            Err(error) => {
                return Err(self.fail_closed_after_cleanup(
                    error.into(),
                    &bundle_dir,
                    Some(&dump_path),
                ));
            }
        };

        let manifest = match phase1_complete(
            self.target(),
            self.require_target_match,
            "artifacts/db.dump",
            checksum_sha256,
            size_bytes,
        ) {
            Ok(manifest) => manifest,
            Err(error) => {
                return Err(self.fail_closed_after_cleanup(error, &bundle_dir, Some(&dump_path)));
            }
        };

        let manifest_path = bundle_dir.join(MANIFEST_FILE_NAME);
        #[cfg(test)]
        INJECT_MANIFEST_WRITE_FAILURE_FOR_TESTS
            .with(|flag| flag.set(self.inject_manifest_write_failure_for_tests));

        if let Err(error) = write_manifest_atomically(&manifest_path, &manifest) {
            return Err(self.fail_closed_after_cleanup(error, &bundle_dir, Some(&dump_path)));
        }

        let canonical_manifest_path = match manifest_path.canonicalize() {
            Ok(canonical_manifest_path) => canonical_manifest_path,
            Err(error) => {
                return Err(self.fail_closed_after_cleanup(
                    error.into(),
                    &bundle_dir,
                    Some(&dump_path),
                ));
            }
        };

        let snapshot_id = Self::build_snapshot_id(&canonical_manifest_path);
        tracing::info!(%snapshot_id, "supabase snapshot created");
        Ok(snapshot_id)
    }

    async fn liveness(&self, snapshot_id: &str) -> Result<Liveness> {
        let manifest_path = Self::parse_snapshot_id(snapshot_id)?;
        let manifest_bytes = match tokio::fs::read(&manifest_path).await {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Liveness::Absent {
                    reason: format!("manifest {} no longer exists", manifest_path.display()),
                });
            }
            Err(error) => {
                return Ok(Liveness::Inconclusive {
                    reason: format!(
                        "could not read manifest {}: {error}",
                        manifest_path.display()
                    ),
                });
            }
        };
        let manifest: SupabaseManifest =
            serde_json::from_slice(&manifest_bytes).map_err(|error| {
                SnapshotError::OperationFailed {
                    action: format!("deserialize supabase manifest {}", manifest_path.display()),
                    detail: error.to_string(),
                }
            })?;

        let dump_path = eligible_dump_path(&manifest, &manifest_path)?;
        Ok(local_file_liveness(&dump_path).await)
    }

    async fn readiness(&self, snapshot_id: &str) -> ArtifactReadiness {
        let dump_path = async {
            let manifest_path = Self::parse_snapshot_id(snapshot_id)?;
            let manifest_bytes = tokio::fs::read(&manifest_path).await?;
            let manifest: SupabaseManifest =
                serde_json::from_slice(&manifest_bytes).map_err(|error| {
                    SnapshotError::OperationFailed {
                        action: "deserialize supabase manifest".to_string(),
                        detail: error.to_string(),
                    }
                })?;
            eligible_dump_path(&manifest, &manifest_path)
        }
        .await;
        match dump_path {
            Ok(dump_path) => pg_dump_readiness(&self.pg_restore_bin, &dump_path).await,
            Err(_) => ArtifactReadiness::UNAVAILABLE,
        }
    }

    async fn rollback(&self, snapshot_id: &str) -> Result<()> {
        let manifest_path = Self::parse_snapshot_id(snapshot_id)?;
        let manifest_bytes =
            fs::read(&manifest_path).map_err(|error| SnapshotError::OperationFailed {
                action: format!("read supabase manifest {}", manifest_path.display()),
                detail: error.to_string(),
            })?;
        let manifest: SupabaseManifest =
            serde_json::from_slice(&manifest_bytes).map_err(|error| {
                SnapshotError::OperationFailed {
                    action: format!("deserialize supabase manifest {}", manifest_path.display()),
                    detail: error.to_string(),
                }
            })?;

        let dump_path = eligible_dump_path(&manifest, &manifest_path)?;
        if !dump_path.exists() {
            return Err(SnapshotError::RollbackDumpNotFound {
                path: dump_path.to_string_lossy().to_string(),
            });
        }

        let expected_sha256 = manifest
            .artifacts
            .db
            .checksum_sha256
            .as_ref()
            .ok_or(SnapshotError::RequiredWhen {
                subject: "manifest artifacts.db.checksum_sha256",
                condition: "for rollback",
            })?
            .clone();
        let actual_sha256 = sha256_hex(&dump_path)?;
        if actual_sha256 != expected_sha256 {
            return Err(SnapshotError::RollbackIntegrityCheckFailed {
                path: dump_path.to_string_lossy().to_string(),
                expected_sha256,
                actual_sha256,
            });
        }

        self.ensure_rollback_target_allowed(&manifest)?;

        let target = manifest
            .target
            .db
            .as_ref()
            .ok_or(SnapshotError::RequiredWhen {
                subject: "manifest target.db",
                condition: "for rollback",
            })?;
        self.run_pg_restore(target, &dump_path).await?;

        tracing::info!(snapshot_id = snapshot_id, "supabase snapshot rolled back");
        Ok(())
    }

    async fn delete(&self, snapshot_id: &str) -> Result<()> {
        let manifest_path = Self::parse_snapshot_id(snapshot_id)?;

        match fs::metadata(&manifest_path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                tracing::info!(path = %manifest_path.display(), "supabase manifest already removed");
                return Ok(());
            }
            Err(error) => {
                return Err(SnapshotError::DeleteFailed {
                    plugin: "supabase".to_string(),
                    snapshot_id: snapshot_id.to_string(),
                    detail: error.to_string(),
                });
            }
            Ok(_) => {}
        }

        let manifest_bytes = match fs::read(&manifest_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                tracing::info!(path = %manifest_path.display(), "supabase manifest already removed");
                return Ok(());
            }
            Err(error) => {
                return Err(SnapshotError::DeleteFailed {
                    plugin: "supabase".to_string(),
                    snapshot_id: snapshot_id.to_string(),
                    detail: error.to_string(),
                });
            }
        };

        let manifest: SupabaseManifest = match serde_json::from_slice(&manifest_bytes) {
            Ok(m) => m,
            Err(error) => {
                return Err(SnapshotError::DeleteFailed {
                    plugin: "supabase".to_string(),
                    snapshot_id: snapshot_id.to_string(),
                    detail: format!("failed to parse manifest: {error}"),
                });
            }
        };

        if let Ok(dump_path) = manifest.resolve_db_artifact_path(&manifest_path) {
            match fs::remove_file(&dump_path) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    return Err(SnapshotError::DeleteFailed {
                        plugin: "supabase".to_string(),
                        snapshot_id: snapshot_id.to_string(),
                        detail: format!("failed to remove dump {}: {error}", dump_path.display()),
                    });
                }
                _ => {}
            }
        }

        match fs::remove_file(&manifest_path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                return Err(SnapshotError::DeleteFailed {
                    plugin: "supabase".to_string(),
                    snapshot_id: snapshot_id.to_string(),
                    detail: format!(
                        "failed to remove manifest {}: {error}",
                        manifest_path.display()
                    ),
                });
            }
            _ => {}
        }

        if let Some(bundle_dir) = manifest_path.parent() {
            match fs::remove_dir(bundle_dir) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {}
                Err(error) => {
                    return Err(SnapshotError::DeleteFailed {
                        plugin: "supabase".to_string(),
                        snapshot_id: snapshot_id.to_string(),
                        detail: format!(
                            "failed to remove bundle {}: {error}",
                            bundle_dir.display()
                        ),
                    });
                }
            }
        }

        tracing::info!(snapshot_id = snapshot_id, "supabase snapshot deleted");
        Ok(())
    }
}
