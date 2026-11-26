use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub fn resolve_shell() -> PathBuf {
    let aegis_real_shell = env::var_os("AEGIS_REAL_SHELL");
    let shell_env = env::var_os("SHELL");
    let current_exe = env::current_exe().ok();
    resolve_shell_inner(
        aegis_real_shell.as_deref(),
        shell_env.as_deref(),
        current_exe.as_deref(),
    )
}

pub fn resolve_shell_inner(
    aegis_real_shell: Option<&OsStr>,
    shell_env: Option<&OsStr>,
    current_exe: Option<&Path>,
) -> PathBuf {
    if let Some(shell) = aegis_real_shell.filter(|s| !s.is_empty()) {
        return PathBuf::from(shell);
    }

    if let Some(shell) = shell_env.filter(|s| !s.is_empty()) {
        let shell_path = PathBuf::from(shell);
        if !same_file(&shell_path, current_exe) {
            return shell_path;
        }
    }

    PathBuf::from("/bin/sh")
}

pub fn same_file(path: &Path, other: Option<&Path>) -> bool {
    let Some(other) = other else {
        return false;
    };

    if path == other {
        return true;
    }

    match (std::fs::canonicalize(path), std::fs::canonicalize(other)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_resolution_aegis_real_shell_takes_priority() {
        let result = resolve_shell_inner(
            Some(std::ffi::OsStr::new("/usr/bin/zsh")),
            Some(std::ffi::OsStr::new("/bin/bash")),
            None,
        );
        assert_eq!(result, PathBuf::from("/usr/bin/zsh"));
    }

    #[test]
    fn shell_resolution_missing_aegis_real_shell_falls_through_to_shell() {
        let result = resolve_shell_inner(None, Some(std::ffi::OsStr::new("/bin/bash")), None);
        assert_eq!(result, PathBuf::from("/bin/bash"));
    }

    #[test]
    fn shell_resolution_shell_pointing_to_aegis_falls_back_to_posix() {
        let aegis_path = PathBuf::from("/usr/local/bin/aegis");
        let result = resolve_shell_inner(None, Some(aegis_path.as_os_str()), Some(&aegis_path));
        assert_eq!(
            result,
            PathBuf::from("/bin/sh"),
            "SHELL pointing to Aegis itself must fall back to /bin/sh"
        );
    }

    #[test]
    fn shell_resolution_invalid_shell_path_returned_as_is() {
        let result =
            resolve_shell_inner(None, Some(std::ffi::OsStr::new("/nonexistent/shell")), None);
        assert_eq!(result, PathBuf::from("/nonexistent/shell"));
    }

    #[test]
    fn shell_resolution_both_missing_falls_back_to_bin_sh() {
        let result = resolve_shell_inner(None, None, None);
        assert_eq!(result, PathBuf::from("/bin/sh"));
    }

    #[test]
    fn same_file_true_for_identical_paths() {
        let p = PathBuf::from("/bin/sh");
        assert!(same_file(&p, Some(&p)));
    }

    #[test]
    fn same_file_false_when_other_is_none() {
        assert!(!same_file(&PathBuf::from("/bin/sh"), None));
    }

    #[test]
    fn same_file_false_for_distinct_paths() {
        assert!(!same_file(
            &PathBuf::from("/bin/sh"),
            Some(&PathBuf::from("/usr/bin/bash"))
        ));
    }

    #[test]
    fn resolve_shell_skips_shell_env_that_is_current_exe() {
        let exe = std::env::current_exe().unwrap();
        let result = resolve_shell_inner(None, Some(exe.as_os_str()), Some(&exe));
        assert_eq!(result, PathBuf::from("/bin/sh"));
    }
}
