//! Linux bubblewrap runtime: program resolution, embedded-build extraction,
//! and namespace probes (ADR-029 §3–§5).
//!
//! Split out of `linux.rs` to keep that file under the 800-line acceptance
//! gate. The public surface here is [`available_bwrap_program`] (used by the
//! exec/spawn paths) and [`wsl1_unavailable`] (used by the caller to name the
//! WSL1 cause in the startup warning).

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[cfg(bwrap_available)]
use std::ffi::CString;

use crate::support::is_forced_sandbox_unavailable;
use crate::{SandboxConfig, SandboxError};

/// Resolve a usable `bwrap` program for `config`, or `None` when the sandbox
/// cannot confine on this host.
///
/// Combines the cached program resolution ([`bwrap_program`]) with the
/// namespace probes: the sysctl gate and a real minimal-sandbox probe matching
/// `allow_network` (catches e.g. WSL2 blocking NETLINK_ROUTE socket creation
/// inside network namespaces). `None` means the command must be refused, never
/// silently run unconfined (ADR-029 §5).
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

/// The resolved `bwrap` program path, cached once per process.
///
/// Prefers a `bwrap` found on `PATH` outside the current working directory
/// (mirrors Codex, which skips a project-local shim), falling back to the
/// embedded bubblewrap build extracted to a memfd. `None` only if neither is
/// usable.
fn bwrap_program() -> Option<&'static PathBuf> {
    static CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();
    CACHE
        .get_or_init(|| find_system_bwrap().or_else(|| extract_embedded_bwrap().ok()))
        .as_ref()
}

/// Find a `bwrap` on `PATH` outside the current working directory that passes
/// the capability smoke check.
fn find_system_bwrap() -> Option<PathBuf> {
    let search_path = std::env::var_os("PATH")?;
    let cwd = std::fs::canonicalize(std::env::current_dir().ok()?).ok()?;
    find_system_bwrap_in_paths(std::env::split_paths(&search_path), &cwd)
}

/// Search `search_paths` for a usable `bwrap`, skipping any that resolves
/// inside `cwd` (a project-local shim, mirroring Codex) or that lacks a
/// production option. Split out so both skip rules are unit-testable without
/// mutating the process cwd.
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

/// The bwrap options the production argv builders can emit, derived from the
/// builders themselves with a maximal config (network on, one writable bind),
/// so the capability check cannot drift from what production actually runs —
/// unlike Codex's probe, which hardcodes `--as-pid-1` and `--perms`.
///
/// [`crate::INNER_LANDLOCK_FLAG`] is excluded: it is the payload marker
/// consumed by the re-exec'd inner wrapper, not a bwrap option.
fn production_bwrap_options() -> Vec<OsString> {
    let maximal = SandboxConfig {
        allow_network: true,
        allow_write: vec![PathBuf::from("/tmp")],
        ..SandboxConfig::default()
    };
    // `/tmp` exists on every Linux host, so this build cannot fail in
    // practice; the empty fallback keeps a hypothetical failure from panicking
    // in production code (the probe then accepts every candidate, and the
    // real namespace probe still gates them).
    let mut args = crate::platform::build_bwrap_args(&maximal).unwrap_or_default();
    if let Ok(wrapper) =
        crate::landlock::build_landlock_wrapper_args(&maximal, OsStr::new("/bin/true"), &[])
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

/// Smoke-check a system `bwrap` candidate: its `--help` output must name every
/// production option, so an ancient or partial bwrap is skipped here — in
/// favor of the next PATH candidate or the embedded build — rather than
/// failing later at exec time with a confusing error. Content is checked, not
/// the exit status: a getopt-style usage banner on stdout still proves flag
/// support, and an unusable binary yields no options to find.
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

/// The embedded bubblewrap executable, compiled by `build.rs` (ADR-029 §3).
#[cfg(bwrap_available)]
static EMBEDDED_BWRAP: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/bwrap"));

/// Materialise the embedded bubblewrap build as an executable memfd and return
/// its `/proc/self/fd/N` path. The fd is intentionally not `CLOEXEC` so it
/// survives the exec of the child; the path is cached by [`bwrap_program`].
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
            let n = unsafe {
                libc::write(
                    fd,
                    bytes[written..].as_ptr() as *const libc::c_void,
                    bytes.len() - written,
                )
            };
            if n < 0 {
                let err = std::io::Error::last_os_error();
                unsafe { libc::close(fd) };
                return Err(SandboxError::Execution(format!(
                    "write to memfd failed: {err}"
                )));
            }
            written += n as usize;
        }

        if unsafe { libc::fchmod(fd, 0o755) } < 0 {
            let err = std::io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(SandboxError::Execution(format!(
                "fchmod on memfd failed: {err}"
            )));
        }

        Ok(PathBuf::from(format!("/proc/self/fd/{fd}")))
    }
}

/// Run a minimal bwrap probe matching `allow_network` to verify namespace
/// creation works on this kernel.
///
/// Binds `/` read-only (the same confinement the real sandbox applies via
/// [`crate::platform::build_bwrap_args`]) so the probe's `-- true` resolves
/// regardless of the caller's `PATH` — which matters when the embedded build is
/// the fallback and `PATH` deliberately omits `bwrap`.
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

// Test-only override for the `/proc/version` content read by
// `wsl1_unavailable`, so the WSL1 verdict is testable on any host. Cleared by
// passing `None`. (A plain comment: `thread_local!` cannot carry doc
// comments.)
#[cfg(test)]
thread_local! {
    static PROC_VERSION_OVERRIDE: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn set_proc_version_override(value: Option<String>) {
    PROC_VERSION_OVERRIDE.with(|cell| *cell.borrow_mut() = value);
}

/// Whether this host is WSL1, which cannot create the user namespaces bwrap
/// needs. Mirrors Codex's detection: `/proc/version` names `wsl1`, or
/// `microsoft` without the `microsoft-standard` (WSL2) marker.
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

/// Whether unprivileged user namespaces are enabled on this kernel. Absent the
/// sysctl file (older kernels), assume they are.
pub(crate) fn sysctl_userns_available() -> bool {
    std::fs::read_to_string("/proc/sys/kernel/unprivileged_userns_clone")
        .map(|v| v.trim() == "1")
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::{find_system_bwrap_in_paths, proc_version_indicates_wsl1};

    #[test]
    fn wsl1_detection_recognizes_wsl1_markers() {
        // WSL1 kernel: "microsoft" without the "microsoft-standard" (WSL2) marker.
        assert!(proc_version_indicates_wsl1(
            "Linux version 4.4.0-19041-Microsoft (Microsoft@Microsoft) ..."
        ));
        // Explicit wsl1 marker.
        assert!(proc_version_indicates_wsl1(
            "Linux version 5.15.90.1-microsoft-standard-WSL1 ..."
        ));
        // WSL2 must NOT be flagged.
        assert!(!proc_version_indicates_wsl1(
            "Linux version 5.15.90.1-microsoft-standard-WSL2 ..."
        ));
        // Native Linux is not WSL at all.
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

    #[test]
    fn system_bwrap_resolver_skips_bwrap_inside_cwd() {
        use std::os::unix::fs::PermissionsExt;

        let base = std::env::temp_dir().join(format!("aegis_bwrap_test_{}", std::process::id()));
        let outside = base.join("outside");
        let inside = base.join("inside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::create_dir_all(&inside).unwrap();

        // A fake bwrap whose --help names every production option (built from
        // the derived option set so the fake cannot drift from the probe).
        let banner = super::production_bwrap_options()
            .iter()
            .map(|option| option.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        let script = format!(
            "#!/bin/sh\ncase \"$1\" in\n  --help) echo '{banner}'; exit 0;;\nesac\nexit 0\n"
        );
        for dir in [&outside, &inside] {
            let path = dir.join("bwrap");
            std::fs::write(&path, &script).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        // cwd = inside: the bwrap inside cwd must be skipped, the outside one found.
        let cwd = std::fs::canonicalize(&inside).unwrap();
        let outside_bwrap = std::fs::canonicalize(outside.join("bwrap")).unwrap();
        let found = find_system_bwrap_in_paths([outside.clone(), inside.clone()], &cwd);
        assert_eq!(found.as_deref(), Some(outside_bwrap.as_path()));

        // cwd = a sibling of base: both search dirs are outside cwd; the first
        // is found. (Using base itself would put both inside cwd and correctly
        // skip them both.)
        let sibling = base.join("sibling");
        std::fs::create_dir_all(&sibling).unwrap();
        let cwd = std::fs::canonicalize(&sibling).unwrap();
        let found = find_system_bwrap_in_paths([outside.clone(), inside.clone()], &cwd);
        assert_eq!(found.as_deref(), Some(outside_bwrap.as_path()));

        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn system_bwrap_resolver_skips_bwrap_missing_production_options() {
        use std::os::unix::fs::PermissionsExt;

        let base =
            std::env::temp_dir().join(format!("aegis_bwrap_caps_test_{}", std::process::id()));
        let crippled = base.join("crippled");
        let complete = base.join("complete");
        std::fs::create_dir_all(&crippled).unwrap();
        std::fs::create_dir_all(&complete).unwrap();
        std::fs::create_dir_all(base.join("cwd")).unwrap();

        // A complete candidate names every derived production option; a
        // crippled one drops the last, standing in for an ancient bwrap that
        // predates it.
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
            let path = dir.join("bwrap");
            std::fs::write(&path, script).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let cwd = std::fs::canonicalize(base.join("cwd")).unwrap();
        let found = find_system_bwrap_in_paths([crippled.clone(), complete.clone()], &cwd);
        let expected = std::fs::canonicalize(complete.join("bwrap")).unwrap();
        assert_eq!(
            found.as_deref(),
            Some(expected.as_path()),
            "a bwrap missing a production option must be skipped for the next candidate"
        );

        // And when every candidate is crippled, none is accepted.
        let found = find_system_bwrap_in_paths([crippled], &cwd);
        assert_eq!(
            found, None,
            "a bwrap missing a production option must be skipped"
        );

        std::fs::remove_dir_all(&base).unwrap();
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
