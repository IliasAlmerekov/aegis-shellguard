#![expect(clippy::unwrap_used, reason = "test helpers fail loudly")]
#![expect(unsafe_code, reason = "FFI calls, each documented by a SAFETY comment")]

#[expect(
    dead_code,
    reason = "this test uses only environment changes; the dedicated probe tests cwd"
)]
#[path = "../../../tests/support/process_state.rs"]
mod process_state;

use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

use aegis_config::SandboxSettings;
use aegis_config::ceiling::{ResolverInputs, TrustedCeiling, compute_trusted_ceiling};
use tempfile::TempDir;

fn paths(c: &TrustedCeiling) -> Vec<PathBuf> {
    c.roots.iter().map(|r| r.path.clone()).collect()
}

fn main() {
    let tmp = TempDir::new().unwrap();
    let base = fs::canonicalize(tmp.path()).unwrap();
    let repo = base.join("repo");
    fs::create_dir_all(repo.join(".git")).unwrap();
    fs::create_dir_all(repo.join("src")).unwrap();
    let mut guard = process_state::ProcessStateGuard::lock();
    let before = compute_trusted_ceiling(&SandboxSettings::default(), &repo);
    // SAFETY: this harness-free executable starts no threads or foreign environment readers.
    unsafe {
        for (key, value) in [
            ("GIT_WORK_TREE", "/"),
            ("GIT_DIR", "/"),
            ("GIT_COMMON_DIR", "/"),
            ("TMPDIR", "/var/tmp"),
        ] {
            guard.set_env(OsStr::new(key), Some(OsStr::new(value)));
        }
    }
    let after = compute_trusted_ceiling(&SandboxSettings::default(), &repo);
    assert_eq!(after, before);
    drop(guard);
    let mut expected = vec![repo];
    expected.extend(ResolverInputs::system().temp_dirs);
    assert_eq!(paths(&after), expected);
}
