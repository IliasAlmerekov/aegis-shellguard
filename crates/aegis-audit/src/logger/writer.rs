use std::env;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::*;
use crate::error::AuditError;
use crate::secure_fs::{create_parent_directories, open_append};
use aegis_config::AuditConfig;

impl Default for AuditLogger {
    fn default() -> Self {
        Self::new(default_audit_path())
    }
}

impl AuditLogger {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            rotation: None,
            integrity_mode: AuditIntegrityMode::ChainSha256,
        }
    }

    #[cfg(any(test, feature = "testing"))]
    pub fn with_path(self, path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            ..self
        }
    }

    pub fn with_rotation(mut self, policy: AuditRotationPolicy) -> Self {
        self.rotation = Some(policy);
        self
    }

    pub fn with_integrity_mode(mut self, mode: AuditIntegrityMode) -> Self {
        self.integrity_mode = mode;
        self
    }

    pub fn from_audit_config(config: &AuditConfig) -> Self {
        let logger = Self::default().with_integrity_mode(config.integrity_mode);
        if let Some(policy) = AuditRotationPolicy::from_config(config) {
            logger.with_rotation(policy)
        } else {
            logger
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use = "audit write failures must be handled — ignoring them silently defeats integrity checking"]
    pub fn append(&self, entry: AuditEntry) -> Result<()> {
        self.append_inner(entry)
            .map_err(|err| self.attach_write_context(err))
    }

    fn attach_write_context(&self, err: AuditError) -> AuditError {
        match err {
            AuditError::Io(source) => AuditError::WriteFailed {
                path: self.path.display().to_string(),
                source,
            },
            other => other,
        }
    }

    fn append_inner(&self, entry: AuditEntry) -> Result<()> {
        create_parent_directories(&self.path)?;
        let _lock = AuditLock::exclusive(&self.lock_path())?;

        let prev_hash = self.latest_chained_hash()?;
        let entry = self.apply_integrity(entry.normalize_legacy_fields(), prev_hash)?;
        let mut serialized =
            serde_json::to_vec(&entry).map_err(|e| AuditError::Io(std::io::Error::other(e)))?;
        serialized.push(b'\n');

        if let Some(policy) = &self.rotation {
            self.rotate_if_needed(policy, serialized.len() as u64)?;
        }

        let mut file = open_append(&self.path)?;

        file.write_all(&serialized)?;
        file.flush()?;
        Ok(())
    }
}

fn default_audit_path() -> PathBuf {
    if let Some(path) = env::var_os("AEGIS_AUDIT_PATH").filter(|value| !value.is_empty()) {
        return PathBuf::from(path);
    }

    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .unwrap_or_else(|| ".".into());
    PathBuf::from(home).join(".aegis").join("audit.jsonl")
}
