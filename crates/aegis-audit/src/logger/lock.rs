use std::fs::File;
use std::path::{Path, PathBuf};

use super::*;
use crate::secure_fs::{create_parent_directories, open_lock, parent_exists_and_is_safe};

impl AuditLock {
    pub(super) fn exclusive(path: &Path) -> Result<Self> {
        let file = open_lock_file(path, true)?;
        file.lock()?;
        Ok(Self { file })
    }

    fn shared(path: &Path) -> Result<Self> {
        let file = open_lock_file(path, false)?;
        file.lock_shared()?;
        Ok(Self { file })
    }
}

impl Drop for AuditLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

impl AuditLogger {
    pub(super) fn lock_path(&self) -> PathBuf {
        let mut file_name = self
            .path
            .file_name()
            .map(|name| name.to_os_string())
            .unwrap_or_else(|| "audit.jsonl".into());
        file_name.push(".lock");

        match self.path.parent() {
            Some(parent) => parent.join(file_name),
            None => PathBuf::from(file_name),
        }
    }

    pub(super) fn acquire_shared_lock(&self) -> Result<Option<AuditLock>> {
        let lock_path = self.lock_path();
        if !parent_exists_and_is_safe(&lock_path)? {
            return Ok(None);
        }

        AuditLock::shared(&lock_path).map(Some)
    }
}

fn open_lock_file(path: &Path, create_parent: bool) -> Result<File> {
    if create_parent {
        create_parent_directories(path)?;
    }

    open_lock(path)
}
