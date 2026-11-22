//! `compute_trusted_ceiling` ignores `GIT_*` and `TMPDIR` environment variables.
//!
//! This file is its own integration-test binary and holds one test, so editing
//! the process environment cannot race another test.

use std::fs;
use std::path::PathBuf;

use aegis_config::SandboxSettings;
use aegis_config::ceiling::{ResolverInputs, TrustedCeiling, compute_trusted_ceiling};
use tempfile::TempDir;

fn paths(c: &TrustedCeiling) -> Vec<PathBuf> {
    c.roots.iter().map(|r| r.path.clone()).collect()
}

#[test]
fn git_and_tmp_environment_variables_do_not_change_the_ceiling() {
    let tmp = TempDir::new().unwrap();
    let base = fs::canonicalize(tmp.path()).unwrap();
    let repo = base.join("repo");
    fs::create_dir_all(repo.join(".git")).unwrap();
    fs::create_dir_all(repo.join("src")).unwrap();

    let before = compute_trusted_ceiling(&SandboxSettings::default(), &repo);
    let keys = ["GIT_WORK_TREE", "GIT_DIR", "GIT_COMMON_DIR", "TMPDIR"];
    let saved: Vec<_> = keys.iter().map(std::env::var_os).collect();
    // SAFETY: this integration-test binary runs only this test, so no other
    // thread reads or mutates the environment.
    unsafe {
        std::env::set_var("GIT_WORK_TREE", "/");
        std::env::set_var("GIT_DIR", "/");
        std::env::set_var("GIT_COMMON_DIR", "/");
        std::env::set_var("TMPDIR", "/var/tmp");
    }
    let after = compute_trusted_ceiling(&SandboxSettings::default(), &repo);
    // SAFETY: this integration-test binary runs only this test, so no other
    // thread reads or mutates the environment.
    unsafe {
        for (key, value) in keys.iter().zip(saved) {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
    assert_eq!(after, before);
    // The OS temp dirs: `/tmp`, plus the per-user one from `confstr` on macOS.
    let mut expected = vec![repo];
    expected.extend(ResolverInputs::system().temp_dirs);
    assert_eq!(paths(&after), expected);
}
