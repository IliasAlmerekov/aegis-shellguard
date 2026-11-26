#![expect(unsafe_code, reason = "FFI calls, each documented by a SAFETY comment")]

#[path = "support/process_state.rs"]
mod process_state;

use std::ffi::OsStr;

struct ProbePanic;

fn run() {
    let cwd = std::env::current_dir().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let original = std::env::var_os("AEGIS_GUARD_PRESENT");
    let absent = std::env::var_os("AEGIS_GUARD_ABSENT");
    assert_eq!(original.as_deref(), Some(OsStr::new("original")));
    assert!(absent.is_none());
    let result = std::panic::catch_unwind(|| {
        let mut guard = process_state::ProcessStateGuard::lock();
        // SAFETY: this executable starts no threads or foreign environment readers.
        unsafe {
            guard.set_env(
                OsStr::new("AEGIS_GUARD_PRESENT"),
                Some(OsStr::new("changed")),
            );
        }
        assert_eq!(
            std::env::var_os("AEGIS_GUARD_PRESENT").as_deref(),
            Some(OsStr::new("changed"))
        );
        // SAFETY: this executable starts no threads or foreign environment readers.
        unsafe {
            guard.set_env(OsStr::new("AEGIS_GUARD_PRESENT"), None);
            guard.set_env(
                OsStr::new("AEGIS_GUARD_ABSENT"),
                Some(OsStr::new("changed")),
            );
        }
        guard.set_cwd(temp.path());
        assert!(std::env::var_os("AEGIS_GUARD_PRESENT").is_none());
        assert_eq!(
            std::env::var_os("AEGIS_GUARD_ABSENT").as_deref(),
            Some(OsStr::new("changed"))
        );
        assert_eq!(
            std::env::current_dir().unwrap(),
            temp.path().canonicalize().unwrap()
        );
        std::panic::panic_any(ProbePanic);
    });
    assert!(result.unwrap_err().is::<ProbePanic>());
    assert_eq!(std::env::var_os("AEGIS_GUARD_PRESENT"), original);
    assert_eq!(std::env::var_os("AEGIS_GUARD_ABSENT"), absent);
    assert_eq!(std::env::current_dir().unwrap(), cwd);
    let result = std::panic::catch_unwind(|| {
        let mut guard = process_state::ProcessStateGuard::lock();
        // SAFETY: this executable starts no threads or foreign environment readers.
        unsafe {
            guard.set_env(OsStr::new("AEGIS_GUARD_PRESENT"), None);
            guard.set_env(OsStr::new("INVALID=KEY"), Some(OsStr::new("x")));
        }
    });
    assert!(result.is_err());
    assert_eq!(std::env::var_os("AEGIS_GUARD_PRESENT"), original);
    let _recovered = process_state::ProcessStateGuard::lock();
}

fn main() {
    const CHILD: &str = "AEGIS_GUARD_PROBE_CHILD";
    if std::env::var_os(CHILD).is_some() {
        run();
    } else {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .env(CHILD, "1")
            .env("AEGIS_GUARD_PRESENT", "original")
            .env_remove("AEGIS_GUARD_ABSENT")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
