//! `compute_trusted_ceiling` ignores `GIT_*` and `TMPDIR` environment variables.

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
    const REPO: &str = "AEGIS_CEILING_ENV_REPO";
    const BEFORE: &str = "AEGIS_CEILING_ENV_BEFORE";
    if let Some(repo) = std::env::var_os(REPO) {
        let repo = PathBuf::from(repo);
        let after = compute_trusted_ceiling(&SandboxSettings::default(), &repo);
        assert_eq!(format!("{after:?}"), std::env::var(BEFORE).unwrap());
        let mut expected = vec![repo];
        expected.extend(ResolverInputs::system().temp_dirs);
        assert_eq!(paths(&after), expected);
    } else {
        let tmp = TempDir::new().unwrap();
        let base = fs::canonicalize(tmp.path()).unwrap();
        let repo = base.join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir_all(repo.join("src")).unwrap();
        let before = compute_trusted_ceiling(&SandboxSettings::default(), &repo);
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                std::thread::current().name().unwrap(),
                "--nocapture",
            ])
            .env(REPO, &repo)
            .env(BEFORE, format!("{before:?}"))
            .env("GIT_WORK_TREE", "/")
            .env("GIT_DIR", "/")
            .env("GIT_COMMON_DIR", "/")
            .env("TMPDIR", "/var/tmp")
            .status()
            .unwrap();
        assert!(status.success());
    }
}
