use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[cfg(bwrap_available)]
use std::ffi::CString;

use crate::support::is_forced_sandbox_unavailable;
use crate::{SandboxConfig, SandboxError};

pub(crate) fn available_bwrap_program(config: &SandboxConfig) -> Option<&'static PathBuf> {
    if is_forced_sandbox_unavailable() {
        return None;
    }
    let program = bwrap_program()?;
    if !sysctl_userns_available() {
        return None;
    }
    if !probe_sandbox_works(program, config.allow_network) {
        return None;
    }
    Some(program)
}

fn bwrap_program() -> Option<&'static PathBuf> {
    static CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();
    CACHE
        .get_or_init(|| find_system_bwrap().or_else(|| extract_embedded_bwrap().ok()))
        .as_ref()
}

fn find_system_bwrap() -> Option<PathBuf> {
    let search_path = std::env::var_os("PATH")?;
    let cwd = std::fs::canonicalize(std::env::current_dir().ok()?).ok()?;
    find_system_bwrap_in_paths(std::env::split_paths(&search_path), &cwd)
}

fn find_system_bwrap_in_paths(
    search_paths: impl IntoIterator<Item = PathBuf>,
    cwd: &Path,
) -> Option<PathBuf> {
    let options = production_bwrap_options();
    let cwd_is_root = cwd.parent().is_none();
    for dir in search_paths {
        let Ok(path) = std::fs::canonicalize(dir.join("bwrap")) else {
            continue;
        };
        if !cwd_is_root && path.starts_with(cwd) {
            continue;
        }
        if bwrap_supports_production_options(&path, &options) {
            return Some(path);
        }
    }
    None
}

fn production_bwrap_options() -> Vec<OsString> {
    let maximal = SandboxConfig {
        allow_network: true,
        allow_write: vec![crate::WriteRoot::new("/tmp")],
        trusted_roots: vec![crate::WriteRoot::new("/tmp")],
    };
    let mut args = crate::platform::build_bwrap_args(&maximal).unwrap_or_default();
    if let Ok(wrapper) = crate::landlock::build_landlock_wrapper_args(OsStr::new("/bin/true"), &[])
    {
        args.extend(wrapper);
    }
    args.into_iter()
        .filter(|arg| {
            let arg = arg.as_os_str();
            arg.to_string_lossy().starts_with("--") && arg != crate::INNER_LANDLOCK_FLAG
        })
        .fold(Vec::new(), |mut options: Vec<OsString>, arg| {
            if !options.contains(&arg) {
                options.push(arg);
            }
            options
        })
}

fn bwrap_supports_production_options(path: &Path, options: &[OsString]) -> bool {
    let Ok(output) = std::process::Command::new(path)
        .arg("--help")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
    else {
        return false;
    };
    let help = String::from_utf8_lossy(&output.stdout);
    options
        .iter()
        .all(|option| help.contains(option.to_string_lossy().as_ref()))
}

#[cfg(bwrap_available)]
static EMBEDDED_BWRAP: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/bwrap"));

fn extract_embedded_bwrap() -> Result<PathBuf, SandboxError> {
    #[cfg(not(bwrap_available))]
    {
        Err(SandboxError::Execution(
            "embedded bwrap not available in this build (AEGIS_SKIP_BWRAP_BUILD)".into(),
        ))
    }

    #[cfg(bwrap_available)]
    {
        let name = CString::new("aegis-bwrap")
            .map_err(|_| SandboxError::Execution("invalid memfd name".into()))?;
        // SAFETY: `name` is a valid NUL-terminated C string that outlives the call.
        let fd = unsafe { libc::memfd_create(name.as_ptr(), 0) };
        if fd < 0 {
            return Err(SandboxError::Execution(format!(
                "memfd_create failed: {}",
                std::io::Error::last_os_error()
            )));
        }

        let bytes = EMBEDDED_BWRAP;
        let mut written = 0usize;
        while written < bytes.len() {
            // SAFETY: `fd` is an open memfd and `bytes[written..]` is valid for `bytes.len() - written` readable bytes.
            let n = unsafe {
                libc::write(
                    fd,
                    bytes[written..].as_ptr() as *const libc::c_void,
                    bytes.len() - written,
                )
            };
            if n < 0 {
                let err = std::io::Error::last_os_error();
                // SAFETY: `fd` is an open memfd owned here and is not used after close.
                unsafe { libc::close(fd) };
                return Err(SandboxError::Execution(format!(
                    "write to memfd failed: {err}"
                )));
            }
            written += n as usize;
        }

        // SAFETY: `fd` is an open memfd owned here.
        if unsafe { libc::fchmod(fd, 0o755) } < 0 {
            let err = std::io::Error::last_os_error();
            // SAFETY: `fd` is an open memfd owned here and is not used after close.
            unsafe { libc::close(fd) };
            return Err(SandboxError::Execution(format!(
                "fchmod on memfd failed: {err}"
            )));
        }

        Ok(PathBuf::from(format!("/proc/self/fd/{fd}")))
    }
}

fn probe_sandbox_works(program: &Path, allow_network: bool) -> bool {
    let mut probe_args: Vec<&str> = vec![
        "--ro-bind",
        "/",
        "/",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--unshare-all",
    ];
    if allow_network {
        probe_args.push("--share-net");
    }
    probe_args.extend(["--", "true"]);

    std::process::Command::new(program)
        .args(&probe_args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
thread_local! {
    static PROC_VERSION_OVERRIDE: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn set_proc_version_override(value: Option<String>) {
    PROC_VERSION_OVERRIDE.with(|cell| *cell.borrow_mut() = value);
}

pub(crate) fn wsl1_unavailable() -> bool {
    #[cfg(test)]
    if let Some(version) = PROC_VERSION_OVERRIDE.with(|cell| cell.borrow().clone()) {
        return proc_version_indicates_wsl1(&version);
    }
    std::fs::read_to_string("/proc/version")
        .map(|v| proc_version_indicates_wsl1(&v))
        .unwrap_or(false)
}

fn proc_version_indicates_wsl1(proc_version: &str) -> bool {
    let proc_version = proc_version.to_ascii_lowercase();
    let mut remaining = proc_version.as_str();
    while let Some(marker) = remaining.find("wsl") {
        let version_start = marker + "wsl".len();
        let version_digits: String = remaining[version_start..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if let Ok(version) = version_digits.parse::<u32>() {
            return version == 1;
        }
        remaining = &remaining[version_start..];
    }
    proc_version.contains("microsoft") && !proc_version.contains("microsoft-standard")
}

#[cfg(test)]
thread_local! {
    static APPARMOR_SYSCTL_PATH: std::cell::RefCell<Option<std::path::PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn set_apparmor_sysctl_path(path: Option<std::path::PathBuf>) {
    APPARMOR_SYSCTL_PATH.with(|value| *value.borrow_mut() = path);
}

pub(crate) fn apparmor_userns_restricted() -> bool {
    let path = std::path::Path::new("/proc/sys/kernel/apparmor_restrict_unprivileged_userns");
    #[cfg(test)]
    let path = APPARMOR_SYSCTL_PATH
        .with(|value| value.borrow().clone())
        .unwrap_or_else(|| path.to_path_buf());
    let content = std::fs::read_to_string(path).ok();
    apparmor_sysctl_restricted(content.as_deref())
}

fn apparmor_sysctl_restricted(content: Option<&str>) -> bool {
    content.is_some_and(|value| value.trim() == "1")
}

pub(crate) fn sysctl_userns_available() -> bool {
    std::fs::read_to_string("/proc/sys/kernel/unprivileged_userns_clone")
        .map(|v| v.trim() == "1")
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::{find_system_bwrap_in_paths, proc_version_indicates_wsl1};

    #[test]
    fn apparmor_public_probe_reads_both_sysctl_states() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apparmor_sysctl");
        super::set_apparmor_sysctl_path(Some(path.clone()));
        std::fs::write(&path, "1\n").unwrap();
        assert!(crate::apparmor_userns_restricted());
        std::fs::write(&path, "0\n").unwrap();
        assert!(!crate::apparmor_userns_restricted());
        std::fs::remove_file(&path).unwrap();
        assert!(!crate::apparmor_userns_restricted());
        super::set_apparmor_sysctl_path(None);
    }

    #[test]
    fn apparmor_restriction_parses_sysctl_content() {
        assert!(super::apparmor_sysctl_restricted(Some("1")));
        assert!(super::apparmor_sysctl_restricted(Some("1\n")));
        assert!(!super::apparmor_sysctl_restricted(Some("0")));
        assert!(!super::apparmor_sysctl_restricted(None));
    }

    #[test]
    fn wsl1_detection_recognizes_wsl1_markers() {
        assert!(proc_version_indicates_wsl1(
            "Linux version 4.4.0-19041-Microsoft (Microsoft@Microsoft) ..."
        ));
        assert!(proc_version_indicates_wsl1(
            "Linux version 5.15.90.1-microsoft-standard-WSL1 ..."
        ));
        assert!(!proc_version_indicates_wsl1(
            "Linux version 5.15.90.1-microsoft-standard-WSL2 ..."
        ));
        assert!(!proc_version_indicates_wsl1(
            "Linux version 6.6.87.2 (gcc ...) #1 SMP ..."
        ));
    }

    #[test]
    fn wsl1_unavailable_reads_the_injected_proc_version() {
        super::set_proc_version_override(Some(
            "Linux version 4.4.0-19041-Microsoft (Microsoft@Microsoft) ...".into(),
        ));
        assert!(
            super::wsl1_unavailable(),
            "an injected WSL1 /proc/version must be detected"
        );

        super::set_proc_version_override(Some(
            "Linux version 5.15.90.1-microsoft-standard-WSL2 ...".into(),
        ));
        assert!(
            !super::wsl1_unavailable(),
            "an injected WSL2 /proc/version must not be detected as WSL1"
        );

        super::set_proc_version_override(None);
    }

    fn write_fake_bwrap(path: &std::path::Path, script: &str) {
        use std::os::unix::fs::PermissionsExt;

        let source = path.with_extension("src");
        std::fs::write(&source, script).unwrap();
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o755)).unwrap();
        let status = std::process::Command::new("cp")
            .arg("-p")
            .arg(&source)
            .arg(path)
            .status()
            .unwrap();
        assert!(status.success(), "cp of the fake bwrap failed: {status}");
    }

    #[test]
    fn production_options_include_the_flags_of_network_and_writable_binds() {
        let options = super::production_bwrap_options();
        for flag in ["--share-net", "--bind"] {
            assert!(
                options.iter().any(|option| option == flag),
                "missing {flag} in {options:?}"
            );
        }
    }

    #[test]
    fn system_bwrap_resolver_skips_bwrap_inside_cwd() {
        let base = std::env::temp_dir().join(format!("aegis_bwrap_test_{}", std::process::id()));
        let outside = base.join("outside");
        let inside = base.join("inside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::create_dir_all(&inside).unwrap();

        let banner = super::production_bwrap_options()
            .iter()
            .map(|option| option.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        let script = format!(
            "#!/bin/sh\ncase \"$1\" in\n  --help) echo '{banner}'; exit 0;;\nesac\nexit 0\n"
        );
        for dir in [&outside, &inside] {
            write_fake_bwrap(&dir.join("bwrap"), &script);
        }

        let cwd = std::fs::canonicalize(&inside).unwrap();
        let outside_bwrap = std::fs::canonicalize(outside.join("bwrap")).unwrap();
        let found = find_system_bwrap_in_paths([outside.clone(), inside.clone()], &cwd);
        assert_eq!(found.as_deref(), Some(outside_bwrap.as_path()));

        let sibling = base.join("sibling");
        std::fs::create_dir_all(&sibling).unwrap();
        let cwd = std::fs::canonicalize(&sibling).unwrap();
        let found = find_system_bwrap_in_paths([outside.clone(), inside.clone()], &cwd);
        assert_eq!(found.as_deref(), Some(outside_bwrap.as_path()));

        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn system_bwrap_resolver_skips_bwrap_missing_production_options() {
        let base =
            std::env::temp_dir().join(format!("aegis_bwrap_caps_test_{}", std::process::id()));
        let crippled = base.join("crippled");
        let complete = base.join("complete");
        std::fs::create_dir_all(&crippled).unwrap();
        std::fs::create_dir_all(&complete).unwrap();
        std::fs::create_dir_all(base.join("cwd")).unwrap();

        let options = super::production_bwrap_options();
        assert!(!options.is_empty(), "derived options must be non-empty");
        let banner = |options: &[std::ffi::OsString]| {
            options
                .iter()
                .map(|option| option.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(" ")
        };
        let complete_script = format!(
            "#!/bin/sh\ncase \"$1\" in\n  --help) echo '{}'; exit 0;;\nesac\nexit 0\n",
            banner(&options)
        );
        let crippled_script = format!(
            "#!/bin/sh\ncase \"$1\" in\n  --help) echo '{}'; exit 0;;\nesac\nexit 0\n",
            banner(&options[..options.len() - 1])
        );

        for (dir, script) in [(&crippled, crippled_script), (&complete, complete_script)] {
            write_fake_bwrap(&dir.join("bwrap"), &script);
        }

        let cwd = std::fs::canonicalize(base.join("cwd")).unwrap();
        let found = find_system_bwrap_in_paths([crippled.clone(), complete.clone()], &cwd);
        let expected = std::fs::canonicalize(complete.join("bwrap")).unwrap();
        assert_eq!(
            found.as_deref(),
            Some(expected.as_path()),
            "a bwrap missing a production option must be skipped for the next candidate"
        );

        let found = find_system_bwrap_in_paths([crippled], &cwd);
        assert_eq!(
            found, None,
            "a bwrap missing a production option must be skipped"
        );

        std::fs::remove_dir_all(&base).unwrap();
    }

    #[cfg(bwrap_available)]
    #[test]
    fn bwrap_program_resolves_to_executable_binary() {
        let path = super::bwrap_program().expect("bwrap must resolve");
        let ok = std::process::Command::new(path)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok, "resolved bwrap program must run --version");
    }

    #[cfg(bwrap_available)]
    #[test]
    fn embedded_bwrap_extracts_to_executable_memfd() {
        let path = super::extract_embedded_bwrap().expect("embedded bwrap must extract");
        let ok = std::process::Command::new(&path)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok, "extracted embedded bwrap must run --version");
    }
}
