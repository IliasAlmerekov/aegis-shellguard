use std::env;
use std::path::PathBuf;

use crate::error::SnapshotError;

type Result<T> = std::result::Result<T, SnapshotError>;

pub(crate) fn resolve_snapshots_dir() -> Result<PathBuf> {
    let home = home_dir().ok_or_else(|| {
        SnapshotError::Config(
            "HOME is not set; cannot determine snapshot storage directory".to_string(),
        )
    })?;
    Ok(home.join(".aegis").join("snapshots"))
}

pub(crate) fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}
