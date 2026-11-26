use std::env;
use std::ffi::OsStr;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub(crate) async fn binary_available(binary: &str) -> bool {
    binary_available_in(binary, env::var_os("PATH").as_deref()).await
}

async fn binary_available_in(binary: &str, path_env: Option<&OsStr>) -> bool {
    if binary.is_empty() {
        return false;
    }

    let binary_path = Path::new(binary);
    if binary_path.components().count() > 1 || binary_path.is_absolute() {
        return is_runnable_file_async(binary_path).await;
    }

    let Some(path_env) = path_env else {
        return false;
    };

    #[cfg(windows)]
    let pathext = env::var_os("PATHEXT")
        .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".into())
        .to_string_lossy()
        .into_owned();

    for path_dir in env::split_paths(path_env) {
        let candidate = path_dir.join(binary);
        if is_runnable_file_async(&candidate).await {
            return true;
        }

        #[cfg(windows)]
        {
            for ext in pathext.split(';') {
                let ext = ext.trim();
                if ext.is_empty() {
                    continue;
                }

                let candidate = path_dir.join(format!("{binary}{ext}"));
                if is_runnable_file_async(&candidate).await {
                    return true;
                }
            }
        }
    }

    false
}

async fn is_runnable_file_async(path: &Path) -> bool {
    let Ok(metadata) = tokio::fs::metadata(path).await else {
        return false;
    };

    metadata_is_runnable_file(metadata)
}

fn metadata_is_runnable_file(metadata: std::fs::Metadata) -> bool {
    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::print_stderr,
        reason = "test child process reports failure on stderr"
    )]

    use std::ffi::OsStr;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use aegis_config::{PostgresSnapshotConfig, SupabaseSnapshotConfig};
    use tempfile::TempDir;

    use super::*;
    use crate::test_support::stub_bin;
    use crate::{MysqlPlugin, PostgresPlugin, SnapshotPlugin, SupabasePlugin};

    #[tokio::test]
    async fn binary_available_in_finds_executable_on_path() {
        let dir = TempDir::new().unwrap();
        stub_bin(dir.path(), "pg_dump", "exit 0");

        assert!(binary_available_in("pg_dump", Some(dir.path().as_os_str())).await);
        assert!(!binary_available_in("mysqldump", Some(dir.path().as_os_str())).await);
    }

    #[tokio::test]
    async fn binary_available_in_rejects_empty_or_missing_path() {
        assert!(!binary_available_in("", Some(OsStr::new("/usr/bin"))).await);
        assert!(!binary_available_in("pg_dump", None).await);
        assert!(!binary_available_in("pg_dump", Some(OsStr::new(""))).await);
    }

    #[tokio::test]
    async fn binary_available_in_rejects_directory_with_same_name() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("pg_dump")).unwrap();

        assert!(!binary_available_in("pg_dump", Some(dir.path().as_os_str())).await);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn binary_available_in_rejects_non_executable_file() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("pg_dump");
        fs::write(&file, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();

        assert!(!binary_available_in("pg_dump", Some(dir.path().as_os_str())).await);
    }

    #[tokio::test]
    async fn binary_available_in_checks_later_path_entries_and_absolute_paths() {
        let first = TempDir::new().unwrap();
        let second = TempDir::new().unwrap();
        let bin = stub_bin(second.path(), "mysqldump", "exit 0");
        let joined = env::join_paths([first.path(), second.path()]).unwrap();

        assert!(binary_available_in("mysqldump", Some(joined.as_os_str())).await);
        assert!(binary_available_in(&bin.to_string_lossy(), None).await);
    }

    const CHILD_PASSED: i32 = 42;
    const CHILD_FAILED: i32 = 43;

    fn run_isolated_child(
        test_fn: &str,
        env_var: &str,
        cwd: Option<&Path>,
        path_override: Option<&Path>,
    ) {
        let module_path = module_path!()
            .split_once("::")
            .map_or(module_path!(), |(_crate_name, rest)| rest);
        let test_name = format!("{module_path}::{test_fn}");

        let exe = env::current_exe().unwrap();
        let mut cmd = std::process::Command::new(exe);
        cmd.args(["--exact", &test_name, "--nocapture"])
            .env(env_var, "1");
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        if let Some(path) = path_override {
            cmd.env("PATH", path);
        }
        let output = cmd.output().expect("failed to re-exec test binary");

        match output.status.code() {
            Some(code) if code == CHILD_PASSED => {}
            Some(0) => {
                panic!("child ran no test for --exact {test_name:?}; check the test filter name")
            }
            Some(code) if code == CHILD_FAILED => panic!(
                "{test_fn} child assertion failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            ),
            other => panic!(
                "child exited unexpectedly ({other:?}):\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
        }
    }

    #[tokio::test]
    async fn binary_available_in_resolves_relative_path_with_separator_without_path_env() {
        if env::var_os("AEGIS_RELATIVE_PATH_CHILD").is_some() {
            let found = binary_available_in("sub/pg_dump", None).await;
            let missing = binary_available_in("sub/missing", None).await;
            if found && !missing {
                std::process::exit(CHILD_PASSED);
            }
            eprintln!("found={found} missing={missing}");
            std::process::exit(CHILD_FAILED);
        }

        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        stub_bin(&sub, "pg_dump", "exit 0");

        run_isolated_child(
            "binary_available_in_resolves_relative_path_with_separator_without_path_env",
            "AEGIS_RELATIVE_PATH_CHILD",
            Some(dir.path()),
            None,
        );
    }

    #[tokio::test]
    async fn providers_find_dump_binaries_on_path_without_which() {
        if env::var_os("AEGIS_BINARY_LOOKUP_CHILD").is_some() {
            let temp = TempDir::new().unwrap();
            let pg = PostgresPlugin::new(
                "app".to_string(),
                "127.0.0.1".to_string(),
                5432,
                "postgres".to_string(),
                temp.path().join("pg"),
            );
            let mysql = MysqlPlugin::new(
                "app".to_string(),
                "127.0.0.1".to_string(),
                3306,
                "root".to_string(),
                temp.path().join("mysql"),
            );
            let supabase = SupabasePlugin::new(
                SupabaseSnapshotConfig {
                    project_ref: "test-ref".to_string(),
                    db: PostgresSnapshotConfig {
                        database: "app".to_string(),
                        host: "127.0.0.1".to_string(),
                        port: 5432,
                        user: "postgres".to_string(),
                    },
                    ..SupabaseSnapshotConfig::default()
                },
                temp.path().join("supabase"),
            );

            let pg_ok = pg.is_applicable(temp.path()).await;
            let mysql_ok = mysql.is_applicable(temp.path()).await;
            let supabase_ok = supabase.is_applicable(temp.path()).await;

            if pg_ok && mysql_ok && supabase_ok {
                std::process::exit(CHILD_PASSED);
            }
            eprintln!("pg_ok={pg_ok} mysql_ok={mysql_ok} supabase_ok={supabase_ok}");
            std::process::exit(CHILD_FAILED);
        }

        let bin_dir = TempDir::new().unwrap();
        stub_bin(bin_dir.path(), "pg_dump", "exit 0");
        stub_bin(bin_dir.path(), "pg_restore", "exit 0");
        stub_bin(bin_dir.path(), "mysqldump", "exit 0");
        assert!(
            !bin_dir.path().join("which").exists(),
            "temp PATH directory must not contain a `which` binary"
        );

        run_isolated_child(
            "providers_find_dump_binaries_on_path_without_which",
            "AEGIS_BINARY_LOOKUP_CHILD",
            None,
            Some(bin_dir.path()),
        );
    }
}
