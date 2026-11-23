//! Restoring guard for process state in single-threaded tests.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

static PROCESS_STATE: Mutex<()> = Mutex::new(());

/// Holds the shared process-state lock and restores changes on drop.
pub struct ProcessStateGuard {
    _lock: MutexGuard<'static, ()>,
    env: Vec<(OsString, Option<OsString>)>,
    cwd: Option<PathBuf>,
}

impl ProcessStateGuard {
    /// Lock process state, including for read-only operations.
    pub fn lock() -> Self {
        Self {
            _lock: PROCESS_STATE
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
            env: Vec::new(),
            cwd: None,
        }
    }

    /// Set or remove a variable until this guard is dropped.
    ///
    /// # Safety
    /// No other thread or foreign library may access the environment until drop.
    #[expect(
        clippy::disallowed_methods,
        reason = "the guard owns environment mutation"
    )]
    pub unsafe fn set_env(&mut self, key: &OsStr, value: Option<&OsStr>) {
        let previous = (key.to_owned(), std::env::var_os(key));
        // SAFETY: the caller excludes all concurrent environment access until drop.
        unsafe {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
        self.env.push(previous);
    }

    /// Change cwd until drop. All code using cwd must coordinate with this lock,
    /// and the original directory must remain available until restoration.
    #[expect(
        clippy::disallowed_methods,
        reason = "the guard owns working-directory mutation"
    )]
    pub fn set_cwd(&mut self, path: &Path) {
        if self.cwd.is_none() {
            self.cwd = Some(std::env::current_dir().unwrap());
        }
        std::env::set_current_dir(path).unwrap();
    }
}

impl Drop for ProcessStateGuard {
    #[expect(
        clippy::disallowed_methods,
        reason = "the guard restores process state"
    )]
    fn drop(&mut self) {
        for (key, previous) in self.env.drain(..).rev() {
            // SAFETY: set_env's caller excludes concurrent access until drop.
            unsafe {
                match previous {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
        if let Some(cwd) = &self.cwd {
            std::env::set_current_dir(cwd).unwrap();
        }
    }
}
