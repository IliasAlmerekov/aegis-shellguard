use std::path::{Component, Path, PathBuf};

use super::super::{Result, SnapshotError, SupabaseManifest, SupabaseTargetDb};

impl SupabaseManifest {
    pub(super) fn resolve_db_artifact_path(&self, manifest_path: &Path) -> Result<PathBuf> {
        let bundle_root = manifest_path.parent().ok_or(SnapshotError::Required {
            subject: "manifest bundle root",
        })?;
        let relative_path = self
            .artifacts
            .db
            .path
            .as_deref()
            .ok_or(SnapshotError::Required {
                subject: "manifest artifacts.db.path",
            })?;
        let artifact_relative = Path::new(relative_path);

        if artifact_relative.is_absolute()
            || artifact_relative.as_os_str().is_empty()
            || artifact_relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(SnapshotError::ManifestArtifactPathRejected {
                reason: "must stay within bundle root",
                path: relative_path.to_string(),
            });
        }

        let bundle_root_canonical = bundle_root.canonicalize()?;
        let resolved = bundle_root.join(artifact_relative);

        let parent =
            resolved
                .parent()
                .ok_or_else(|| SnapshotError::ManifestArtifactPathRejected {
                    reason: "has no parent under bundle root",
                    path: relative_path.to_string(),
                })?;
        let parent_canonical = match parent.canonicalize() {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(resolved),
            Err(error) => {
                return Err(SnapshotError::OperationFailed {
                    action: "resolve artifact parent for rollback".to_string(),
                    detail: error.to_string(),
                });
            }
        };

        if !parent_canonical.starts_with(&bundle_root_canonical) {
            return Err(SnapshotError::ManifestArtifactPathRejected {
                reason: "escapes bundle root",
                path: relative_path.to_string(),
            });
        }

        if resolved.exists() {
            let canonical_resolved =
                resolved
                    .canonicalize()
                    .map_err(|error| SnapshotError::OperationFailed {
                        action: "canonicalize rollback artifact path".to_string(),
                        detail: error.to_string(),
                    })?;

            if !canonical_resolved.starts_with(&bundle_root_canonical) {
                return Err(SnapshotError::ManifestArtifactPathRejected {
                    reason: "resolves outside bundle root",
                    path: relative_path.to_string(),
                });
            }

            return Ok(canonical_resolved);
        }

        Ok(resolved)
    }

    pub(super) fn ensure_target_matches(&self, configured: &SupabaseTargetDb) -> Result<()> {
        let target_db = self.target.db.as_ref().ok_or(SnapshotError::RequiredWhen {
            subject: "manifest target.db",
            condition: "for rollback",
        })?;

        let mut mismatches = Vec::new();

        if target_db.database != configured.database {
            mismatches.push(format!(
                "database manifest={} current={}",
                target_db.database, configured.database
            ));
        }
        if target_db.host != configured.host {
            mismatches.push(format!(
                "host manifest={} current={}",
                target_db.host, configured.host
            ));
        }
        if target_db.port != configured.port {
            mismatches.push(format!(
                "port manifest={} current={}",
                target_db.port, configured.port
            ));
        }
        if target_db.user != configured.user {
            mismatches.push(format!(
                "user manifest={} current={}",
                target_db.user, configured.user
            ));
        }

        if mismatches.is_empty() {
            Ok(())
        } else {
            Err(SnapshotError::RollbackTargetMismatch {
                details: mismatches.join(", "),
            })
        }
    }
}
