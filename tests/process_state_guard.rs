#[allow(dead_code)]
#[path = "support/process_state.rs"]
mod process_state;

use process_state::{CwdGuard, EnvVarGuard};
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
        // SAFETY: this executable starts no threads and no other code accesses its environment.
        let _present = unsafe {
            EnvVarGuard::new(
                OsStr::new("AEGIS_GUARD_PRESENT"),
                Some(OsStr::new("changed")),
            )
        };
        assert_eq!(
            std::env::var_os("AEGIS_GUARD_PRESENT").as_deref(),
            Some(OsStr::new("changed"))
        );
        std::panic::panic_any(ProbePanic);
    });
    assert!(result.unwrap_err().is::<ProbePanic>());
    assert_eq!(std::env::var_os("AEGIS_GUARD_PRESENT"), original);
    let result = std::panic::catch_unwind(|| {
        // SAFETY: this executable starts no threads and no other code accesses its environment.
        let _removed = unsafe { EnvVarGuard::new(OsStr::new("AEGIS_GUARD_PRESENT"), None) };
        assert!(std::env::var_os("AEGIS_GUARD_PRESENT").is_none());
        std::panic::panic_any(ProbePanic);
    });
    assert!(result.unwrap_err().is::<ProbePanic>());
    assert_eq!(std::env::var_os("AEGIS_GUARD_PRESENT"), original);
    let result = std::panic::catch_unwind(|| {
        let _cwd = CwdGuard::new(temp.path());
        assert_eq!(
            std::env::current_dir().unwrap(),
            temp.path().canonicalize().unwrap()
        );
        std::panic::panic_any(ProbePanic);
    });
    assert!(result.unwrap_err().is::<ProbePanic>());
    assert_eq!(std::env::current_dir().unwrap(), cwd);
    let result = std::panic::catch_unwind(|| {
        // SAFETY: this executable starts no threads and no other code accesses its environment.
        let _absent = unsafe {
            EnvVarGuard::new(
                OsStr::new("AEGIS_GUARD_ABSENT"),
                Some(OsStr::new("changed")),
            )
        };
        assert_eq!(
            std::env::var_os("AEGIS_GUARD_ABSENT").as_deref(),
            Some(OsStr::new("changed"))
        );
        std::panic::panic_any(ProbePanic);
    });
    assert!(result.unwrap_err().is::<ProbePanic>());
    assert_eq!(std::env::var_os("AEGIS_GUARD_ABSENT"), absent);
}

fn main() {
    const CHILD: &str = "AEGIS_GUARD_PROBE_CHILD";
    if std::env::var_os(CHILD).is_some() {
        run();
    } else {
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .env(CHILD, "1")
                .env("AEGIS_GUARD_PRESENT", "original")
                .env_remove("AEGIS_GUARD_ABSENT")
                .status()
                .unwrap()
                .success()
        );
    }
}
