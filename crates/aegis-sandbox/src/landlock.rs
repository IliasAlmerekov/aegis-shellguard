//! Landlock LSM syscalls and the inner re-exec landlock wrapper.
//!
//! Landlock is applied in the innermost process inside bwrap's mount namespace
//! (see [`run_inner_landlock_wrapper`]) so the bwrap + Landlock layers compose
//! instead of conflicting. This module owns the raw syscall wrappers, the strict
//! ruleset application, and the thin re-exec wrapper that `main.rs` dispatches
//! to. It is Linux-only.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use crate::{SandboxConfig, SandboxError};

// ── Landlock syscalls ─────────────────────────────────────────────────────────

mod syscalls {
    // Landlock filesystem access rights (from linux/landlock.h).
    pub const ACCESS_FS_WRITE_FILE: u64 = 1 << 1;
    pub const ACCESS_FS_REMOVE_DIR: u64 = 1 << 4;
    pub const ACCESS_FS_REMOVE_FILE: u64 = 1 << 5;
    pub const ACCESS_FS_MAKE_CHAR: u64 = 1 << 6;
    pub const ACCESS_FS_MAKE_DIR: u64 = 1 << 7;
    pub const ACCESS_FS_MAKE_REG: u64 = 1 << 8;
    pub const ACCESS_FS_MAKE_SOCK: u64 = 1 << 9;
    pub const ACCESS_FS_MAKE_FIFO: u64 = 1 << 10;
    pub const ACCESS_FS_MAKE_BLOCK: u64 = 1 << 11;
    pub const ACCESS_FS_MAKE_SYM: u64 = 1 << 12;
    /// ABI 2+ only.
    pub const ACCESS_FS_REFER: u64 = 1 << 13;
    /// ABI 3+ only.
    pub const ACCESS_FS_TRUNCATE: u64 = 1 << 14;

    /// All write-related accesses supported in ABI 1 (baseline).
    pub const ALL_WRITE_V1: u64 = ACCESS_FS_WRITE_FILE
        | ACCESS_FS_REMOVE_DIR
        | ACCESS_FS_REMOVE_FILE
        | ACCESS_FS_MAKE_CHAR
        | ACCESS_FS_MAKE_DIR
        | ACCESS_FS_MAKE_REG
        | ACCESS_FS_MAKE_SOCK
        | ACCESS_FS_MAKE_FIFO
        | ACCESS_FS_MAKE_BLOCK
        | ACCESS_FS_MAKE_SYM;

    pub const RULE_PATH_BENEATH: u32 = 1;
    /// Flag for `landlock_create_ruleset` that returns the ABI version instead
    /// of creating a ruleset.
    pub const CREATE_RULESET_VERSION: u32 = 1;

    #[repr(C)]
    pub struct RulesetAttr {
        pub handled_access_fs: u64,
        pub handled_access_net: u64,
    }

    #[repr(C)]
    pub struct PathBeneathAttr {
        pub allowed_access: u64,
        pub parent_fd: i32,
    }

    /// Return the Landlock ABI version supported by this kernel, or 0 if
    /// Landlock is not available (kernel < 5.13 or not compiled in).
    pub fn detect_abi() -> u32 {
        // SAFETY: SYS_landlock_create_ruleset with the version flag is a
        // read-only query syscall that cannot cause side effects.
        let ret = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                std::ptr::null::<RulesetAttr>(),
                0usize,
                CREATE_RULESET_VERSION,
            )
        };
        if ret < 0 { 0 } else { ret as u32 }
    }

    pub fn create_ruleset(
        attr: &RulesetAttr,
        size: usize,
    ) -> std::io::Result<std::os::unix::io::OwnedFd> {
        use std::os::unix::io::FromRawFd;
        // SAFETY: SYS_landlock_create_ruleset creates a new file descriptor; we
        // take ownership via OwnedFd if the call succeeds.
        let fd = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                attr as *const _ as *const libc::c_void,
                size,
                0u32,
            )
        };
        if fd < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(unsafe { std::os::unix::io::OwnedFd::from_raw_fd(fd as std::os::unix::io::RawFd) })
        }
    }

    pub fn add_path_beneath(
        ruleset_fd: std::os::unix::io::BorrowedFd<'_>,
        attr: &PathBeneathAttr,
    ) -> std::io::Result<()> {
        use std::os::unix::io::AsRawFd;
        // SAFETY: valid file descriptors and well-formed attr struct.
        let ret = unsafe {
            libc::syscall(
                libc::SYS_landlock_add_rule,
                ruleset_fd.as_raw_fd(),
                RULE_PATH_BENEATH,
                attr as *const _ as *const libc::c_void,
                0u32,
            )
        };
        if ret != 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    pub fn restrict_self(ruleset_fd: std::os::unix::io::BorrowedFd<'_>) -> std::io::Result<()> {
        use std::os::unix::io::AsRawFd;
        // SAFETY: valid file descriptor; restricts the calling thread.
        let ret = unsafe {
            libc::syscall(
                libc::SYS_landlock_restrict_self,
                ruleset_fd.as_raw_fd(),
                0u32,
            )
        };
        if ret != 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

// ── Ruleset application ───────────────────────────────────────────────────────

/// Apply Landlock filesystem write restrictions described by `config`.
///
/// When `allow_write` is empty, no restrictions are applied. When Landlock is
/// not supported by the kernel (ENOSYS, ABI 0), the function fails closed: the
/// caller configured a write restriction that cannot be enforced, so the
/// command must not run unconfined. Sets `PR_SET_NO_NEW_PRIVS` before
/// `landlock_restrict_self`, which the kernel requires for an unprivileged
/// process.
pub(crate) fn apply_landlock_restrictions(config: &SandboxConfig) -> Result<(), SandboxError> {
    // Nothing to restrict if no write paths are configured.
    if config.allow_write.is_empty() {
        return Ok(());
    }

    let abi = syscalls::detect_abi();
    if abi == 0 {
        // Kernel < 5.13 or Landlock not compiled in. Fail closed.
        return Err(SandboxError::Execution(
            "landlock unavailable (ABI 0) but allow_write is non-empty".into(),
        ));
    }

    // The kernel requires no_new_privs before landlock_restrict_self for an
    // unprivileged process. Set it first (one-way flag).
    // SAFETY: PR_SET_NO_NEW_PRIVS is a benign, documented prctl.
    let nnp = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
    if nnp != 0 {
        return Err(SandboxError::Execution(format!(
            "prctl(PR_SET_NO_NEW_PRIVS): {}",
            std::io::Error::last_os_error()
        )));
    }

    // Build handled_access_fs mask for the detected ABI.
    let mut handled_fs = syscalls::ALL_WRITE_V1;
    if abi >= 2 {
        handled_fs |= syscalls::ACCESS_FS_REFER;
    }
    if abi >= 3 {
        handled_fs |= syscalls::ACCESS_FS_TRUNCATE;
    }

    let attr = syscalls::RulesetAttr {
        handled_access_fs: handled_fs,
        handled_access_net: 0,
    };
    let size = std::mem::size_of::<syscalls::RulesetAttr>();

    let ruleset = syscalls::create_ruleset(&attr, size)
        .map_err(|e| SandboxError::Execution(format!("landlock create_ruleset: {e}")))?;

    for path in &config.allow_write {
        let canonical = path.canonicalize().map_err(|e| {
            SandboxError::Execution(format!("canonicalize {}: {e}", path.display()))
        })?;
        let dir_file = std::fs::File::open(&canonical)
            .map_err(|e| SandboxError::Execution(format!("open {}: {e}", canonical.display())))?;

        use std::os::unix::io::{AsFd, AsRawFd};
        let rule_attr = syscalls::PathBeneathAttr {
            allowed_access: handled_fs,
            parent_fd: dir_file.as_raw_fd(),
        };
        syscalls::add_path_beneath(ruleset.as_fd(), &rule_attr)
            .map_err(|e| SandboxError::Execution(format!("landlock add_rule: {e}")))?;
    }

    use std::os::unix::io::AsFd;
    syscalls::restrict_self(ruleset.as_fd())
        .map_err(|e| SandboxError::Execution(format!("landlock restrict_self: {e}")))?;

    Ok(())
}

// ── Inner landlock wrapper (re-exec) ─────────────────────────────────────────

/// Env var carrying the length-prefixed `allow_write` paths to the inner
/// landlock wrapper.
pub(crate) const AEGIS_LANDLOCK_ALLOW_WRITE: &str = "AEGIS_LANDLOCK_ALLOW_WRITE";

/// Upper bound for the serialized `allow_write` env var, matching the kernel's
/// per-argument limit (`MAX_ARG_STRLEN` = 128 KiB). Exceeding it fails closed
/// at prepare time rather than silently truncating.
pub(crate) const MAX_ALLOW_WRITE_ENV_LEN: usize = 128 * 1024;

/// Serialize `allow_write` paths as a length-prefixed byte string:
/// `<len>:<bytes>` per path, using raw `OsStr` bytes so any path content
/// (spaces, colons, non-UTF-8) round-trips losslessly.
pub(crate) fn serialize_allow_write(paths: &[PathBuf]) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    let mut out = Vec::new();
    for p in paths {
        let bytes = p.as_os_str().as_bytes();
        out.extend_from_slice(bytes.len().to_string().as_bytes());
        out.push(b':');
        out.extend_from_slice(bytes);
    }
    out
}

/// Inverse of [`serialize_allow_write`]. Returns an error on malformed input so
/// the inner wrapper can fail closed rather than run the command unconfined.
pub(crate) fn parse_allow_write(encoded: &[u8]) -> Result<Vec<PathBuf>, SandboxError> {
    use std::os::unix::ffi::OsStringExt;
    let mut paths = Vec::new();
    let mut rest = encoded;
    while !rest.is_empty() {
        let colon = rest
            .iter()
            .position(|&b| b == b':')
            .ok_or_else(|| SandboxError::Execution("malformed allow_write encoding".into()))?;
        let len: usize = std::str::from_utf8(&rest[..colon])
            .ok()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| SandboxError::Execution("malformed allow_write length".into()))?;
        let start = colon + 1;
        if start + len > rest.len() {
            return Err(SandboxError::Execution(
                "malformed allow_write length".into(),
            ));
        }
        paths.push(PathBuf::from(OsString::from_vec(
            rest[start..start + len].to_vec(),
        )));
        rest = &rest[start + len..];
    }
    Ok(paths)
}

/// Build the inner landlock wrapper command args to append after the bwrap
/// namespace args: `[--ro-bind <exe> <exe>, <exe>, <flag>, <program>, <args…>]`.
///
/// The resolved `current_exe` is bound read-only into the namespace so the
/// re-exec'd binary is visible, then invoked with the internal flag followed by
/// the real program and its args. The inner wrapper applies Landlock and execs
/// the program. Canonicalizes each `allow_write` path up front so a missing
/// path fails closed at prepare time.
pub(crate) fn build_landlock_wrapper_args(
    config: &SandboxConfig,
    program: &OsStr,
    args: &[OsString],
) -> Result<Vec<OsString>, SandboxError> {
    for path in &config.allow_write {
        path.canonicalize().map_err(|e| {
            SandboxError::Execution(format!("allow_write path {}: {e}", path.display()))
        })?;
    }

    let exe = std::env::current_exe()
        .map_err(|e| SandboxError::Execution(format!("current_exe: {e}")))?;

    let mut out = vec![
        OsString::from("--ro-bind"),
        exe.clone().into_os_string(),
        exe.clone().into_os_string(),
        exe.into_os_string(),
        OsString::from(crate::INNER_LANDLOCK_FLAG),
        program.to_owned(),
    ];
    out.extend_from_slice(args);
    Ok(out)
}

/// Run the inner landlock wrapper (the re-exec'd innermost process inside
/// bwrap's mount namespace). Reads the serialized `allow_write` config from
/// the environment, applies Landlock, then execs the real program from argv.
///
/// Fail-closed: any error — including a malformed config or Landlock ABI 0 —
/// exits non-zero before the target command runs. There is no branch where the
/// inner process executes the command unconfined.
pub(crate) fn run_inner_landlock_wrapper() -> i32 {
    use std::os::unix::ffi::OsStringExt;

    let encoded = match std::env::var_os(AEGIS_LANDLOCK_ALLOW_WRITE) {
        Some(v) => v.into_vec(),
        None => {
            eprintln!("aegis: inner landlock wrapper: missing {AEGIS_LANDLOCK_ALLOW_WRITE}");
            return 1;
        }
    };
    let allow_write = match parse_allow_write(&encoded) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("aegis: inner landlock wrapper: {e}");
            return 1;
        }
    };
    let config = SandboxConfig {
        allow_write,
        ..Default::default()
    };
    if let Err(e) = apply_landlock_restrictions(&config) {
        eprintln!("aegis: failed to apply landlock: {e}");
        return 1;
    }

    // argv after the flag: [program, args...].
    let argv: Vec<OsString> = std::env::args_os().skip(2).collect();
    let Some(program) = argv.first() else {
        eprintln!("aegis: inner landlock wrapper: no program to exec");
        return 1;
    };

    use std::os::unix::process::CommandExt;
    let err = std::process::Command::new(program).args(&argv[1..]).exec();
    eprintln!("aegis: failed to exec {program:?}: {err}");
    1
}

/// Return the Landlock ABI version supported by this kernel (0 if unavailable).
pub(crate) fn landlock_abi() -> u32 {
    syscalls::detect_abi()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::SandboxConfig;

    use super::{
        apply_landlock_restrictions, build_landlock_wrapper_args, parse_allow_write,
        serialize_allow_write,
    };

    #[test]
    fn apply_landlock_restrictions_ok_on_empty_allow_write() {
        // No write paths → no Landlock ruleset created → Ok(()).
        assert!(apply_landlock_restrictions(&SandboxConfig::default()).is_ok());
    }

    #[test]
    fn apply_landlock_restrictions_fails_closed_when_abi_zero() {
        // Only meaningful on hosts without Landlock (ABI 0). On ABI >= 1 hosts
        // the ruleset would apply and permanently restrict this test process,
        // so we skip there; the end-to-end test covers the ABI >= 1 path.
        if super::landlock_abi() != 0 {
            return;
        }
        let cfg = SandboxConfig {
            allow_write: vec![PathBuf::from("/tmp")],
            ..Default::default()
        };
        assert!(
            apply_landlock_restrictions(&cfg).is_err(),
            "non-empty allow_write on ABI 0 must fail closed"
        );
    }

    #[test]
    fn allow_write_serialization_round_trips_utf8_and_tricky_paths() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let paths = vec![
            PathBuf::from("/workspace"),
            PathBuf::from("/tmp/with space"),
            PathBuf::from("/tmp/colon:and:colons"),
            PathBuf::from(OsString::from_vec(b"/tmp/non-utf8-\xff".to_vec())),
        ];
        let encoded = serialize_allow_write(&paths);
        let decoded = parse_allow_write(&encoded).expect("round-trip parse must succeed");
        assert_eq!(decoded, paths);
    }

    #[test]
    fn allow_write_serialization_empty_round_trips() {
        let encoded = serialize_allow_write(&[]);
        assert!(encoded.is_empty());
        assert!(
            parse_allow_write(&encoded)
                .expect("empty parse must succeed")
                .is_empty()
        );
    }

    #[test]
    fn build_landlock_wrapper_args_emits_ro_bind_and_inner_command() {
        use std::ffi::{OsStr, OsString};

        let cfg = SandboxConfig {
            allow_write: vec![PathBuf::from("/tmp")],
            ..Default::default()
        };
        let exe = std::env::current_exe().expect("current_exe must resolve in tests");
        let args = build_landlock_wrapper_args(
            &cfg,
            OsStr::new("/bin/sh"),
            &[OsString::from("-c"), OsString::from("echo hi")],
        )
        .expect("build_landlock_wrapper_args must succeed for /tmp");

        // [--ro-bind <exe> <exe>, <exe>, <flag>, <program>, <args...>]
        assert_eq!(args[0].as_os_str(), "--ro-bind");
        assert_eq!(args[1].as_os_str(), exe.as_os_str());
        assert_eq!(args[2].as_os_str(), exe.as_os_str());
        assert_eq!(args[3].as_os_str(), exe.as_os_str());
        assert_eq!(args[4].as_os_str(), crate::INNER_LANDLOCK_FLAG);
        assert_eq!(args[5].as_os_str(), "/bin/sh");
        assert_eq!(args[6].as_os_str(), "-c");
        assert_eq!(args[7].as_os_str(), "echo hi");
    }

    #[test]
    fn build_landlock_wrapper_args_fails_for_nonexistent_allow_write() {
        use std::ffi::OsStr;

        let cfg = SandboxConfig {
            allow_write: vec![PathBuf::from("/nonexistent_aegis_test_path_xyz")],
            ..Default::default()
        };
        assert!(
            build_landlock_wrapper_args(&cfg, OsStr::new("/bin/sh"), &[]).is_err(),
            "expected Err for non-existent allow_write path"
        );
    }

    #[test]
    fn test_landlock_stub_is_callable() {
        assert!(apply_landlock_restrictions(&SandboxConfig::default()).is_ok());
    }

    #[test]
    fn parse_allow_write_rejects_malformed_encoding() {
        // A length that overruns the buffer must fail closed.
        assert!(parse_allow_write(b"999:/tmp").is_err());
        // A non-numeric length must fail closed.
        assert!(parse_allow_write(b"xx:/tmp").is_err());
    }
}
