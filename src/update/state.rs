//! Global update-check state under `~/.aegis/update.json`.
//!
//! Never read or written from a project `.aegis.toml` layer — consent to an
//! outbound request is a per-machine decision, not something a project
//! checked into a shared repository can turn on for a developer.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::error::UpdateError;

type Result<T> = std::result::Result<T, UpdateError>;

const AEGIS_DIR_NAME: &str = ".aegis";
const STATE_FILE_NAME: &str = "update.json";
const LOCK_FILE_NAME: &str = "update.lock";

/// An installation channel Aegis can check for updates. Only `Npm` exists in
/// v1 (ADR-038); the enum shape leaves room for a future channel without a
/// breaking change to the state schema or CLI surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    /// The npm registry entry for `@iliasalmerekov/aegis`.
    Npm,
}

impl Channel {
    pub(super) fn from_arg(raw: &str) -> Option<Self> {
        match raw {
            "npm" => Some(Self::Npm),
            _ => None,
        }
    }

    pub(super) fn as_arg(self) -> &'static str {
        match self {
            Self::Npm => "npm",
        }
    }
}

impl std::fmt::Display for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_arg())
    }
}

/// Consent, cache, and notice-throttling state persisted to
/// `~/.aegis/update.json`.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct UpdateState {
    /// Selected installation channel. `None` when never enabled or after
    /// `disable`.
    #[serde(default)]
    pub channel: Option<Channel>,
    /// Whether the user opted in to update checks.
    #[serde(default)]
    pub consent: bool,
    /// The newest version the last successful check observed.
    #[serde(default)]
    pub latest_known_version: Option<String>,
    /// RFC 3339 timestamp of the last successful registry check.
    #[serde(default)]
    pub last_success_check_at: Option<String>,
    /// The version the update notice was last shown for.
    #[serde(default)]
    pub last_notice_version: Option<String>,
    /// RFC 3339 timestamp the notice was last shown at, for
    /// `last_notice_version`.
    #[serde(default)]
    pub last_notice_at: Option<String>,
}

fn home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or(UpdateError::NoHome)
}

/// Resolve `~/.aegis/update.json`.
pub fn state_path() -> Result<PathBuf> {
    Ok(home_dir()?.join(AEGIS_DIR_NAME).join(STATE_FILE_NAME))
}

/// Resolve `~/.aegis/update.lock`, the background-check dedup lock.
pub fn lock_path() -> Result<PathBuf> {
    Ok(home_dir()?.join(AEGIS_DIR_NAME).join(LOCK_FILE_NAME))
}

/// Load state from `path`. A missing file is not an error — it is the
/// default, not-yet-configured state.
pub fn load(path: &Path) -> Result<UpdateState> {
    match fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str(&raw).map_err(|err| UpdateError::CorruptState {
            path: path.display().to_string(),
            detail: err.to_string(),
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(UpdateState::default()),
        Err(err) => Err(err.into()),
    }
}

/// Persist `state` to `path` atomically (temp file + rename), owner-only on
/// Unix. A failure here must never corrupt the previous valid file — the
/// rename only happens once the temp file is fully written and synced.
pub fn save(path: &Path, state: &UpdateState) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let body = serde_json::to_string_pretty(state).map_err(|err| UpdateError::Internal {
        detail: format!("failed to serialize update state: {err}"),
    })?;

    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp_path = path.with_extension(format!("tmp.{pid}.{nanos}"));

    {
        let file = fs::File::create(&tmp_path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        use std::io::Write;
        let mut file = file;
        file.write_all(body.as_bytes())?;
        file.sync_all()?;
    }

    fs::rename(&tmp_path, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp_path);
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn missing_state_file_loads_as_default() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(STATE_FILE_NAME);

        let state = load(&path).unwrap();

        assert_eq!(state, UpdateState::default());
        assert!(!state.consent);
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(STATE_FILE_NAME);
        let state = UpdateState {
            channel: Some(Channel::Npm),
            consent: true,
            latest_known_version: Some("0.6.8".to_string()),
            ..UpdateState::default()
        };

        save(&path, &state).unwrap();
        let loaded = load(&path).unwrap();

        assert_eq!(loaded, state);
    }

    #[test]
    fn corrupt_state_file_is_a_distinct_error_not_silently_defaulted() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(STATE_FILE_NAME);
        fs::write(&path, "not json").unwrap();

        let err = load(&path).unwrap_err();

        assert!(matches!(err, UpdateError::CorruptState { .. }));
    }

    #[test]
    fn save_writes_owner_only_permissions() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir = TempDir::new().unwrap();
            let path = dir.path().join(STATE_FILE_NAME);

            save(&path, &UpdateState::default()).unwrap();

            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn save_creates_missing_parent_directories() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nested").join(STATE_FILE_NAME);

        save(&path, &UpdateState::default()).unwrap();

        assert!(path.exists());
    }

    #[test]
    fn channel_arg_round_trips() {
        assert_eq!(Channel::from_arg("npm"), Some(Channel::Npm));
        assert_eq!(Channel::from_arg("bogus"), None);
        assert_eq!(Channel::Npm.as_arg(), "npm");
    }
}
