//! Restoring guards for process state in tests.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

static PROCESS_STATE: Mutex<()> = Mutex::new(());

/// Restores one environment variable when dropped.
pub struct EnvVarGuard {
    key: OsString,
    previous: Option<OsString>,
    _lock: MutexGuard<'static, ()>,
}

impl EnvVarGuard {
    /// Set or remove a variable until the guard is dropped.
    ///
    /// # Safety
    /// No other thread or library may access the process environment while this guard lives.
    #[expect(
        clippy::disallowed_methods,
        reason = "the guard owns environment mutation"
    )]
    pub unsafe fn new(key: &OsStr, value: Option<&OsStr>) -> Self {
        let lock = PROCESS_STATE
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let previous = std::env::var_os(key);
        // SAFETY: the caller excludes all concurrent environment access for the guard's lifetime.
        unsafe {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
        Self {
            key: key.to_owned(),
            previous,
            _lock: lock,
        }
    }
}

impl Drop for EnvVarGuard {
    #[expect(
        clippy::disallowed_methods,
        reason = "the guard restores environment state"
    )]
    fn drop(&mut self) {
        // SAFETY: the constructor's caller excludes concurrent access until this guard is dropped.
        unsafe {
            match &self.previous {
                Some(value) => std::env::set_var(&self.key, value),
                None => std::env::remove_var(&self.key),
            }
        }
    }
}

/// Restores the working directory when dropped.
pub struct CwdGuard {
    previous: PathBuf,
    _lock: MutexGuard<'static, ()>,
}

impl CwdGuard {
    /// Change the working directory until the guard is dropped.
    #[expect(
        clippy::disallowed_methods,
        reason = "the guard owns working-directory mutation"
    )]
    pub fn new(path: &Path) -> Self {
        let lock = PROCESS_STATE
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(path).unwrap();
        Self {
            previous,
            _lock: lock,
        }
    }
}

impl Drop for CwdGuard {
    #[expect(
        clippy::disallowed_methods,
        reason = "the guard restores the working directory"
    )]
    fn drop(&mut self) {
        std::env::set_current_dir(&self.previous).unwrap();
    }
}

/// Run the current test in a child so unguarded cwd readers stay isolated.
pub fn isolated(f: impl FnOnce()) {
    const CHILD: &str = "AEGIS_PROCESS_STATE_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        f();
    } else {
        let name = std::thread::current().name().unwrap().to_owned();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &name, "--nocapture"])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(status.success());
    }
}
