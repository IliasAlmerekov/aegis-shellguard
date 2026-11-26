#![expect(unsafe_code, reason = "libc calls are sandbox/system boundaries, each documented by a SAFETY comment")]

#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable
    )
)]

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use aegis_types::{CeilingLayer, SandboxStatus};

mod support;

pub use support::{OmitReason, OmittedRoot, ResolvedAllowWrite, resolve_allow_write};

#[cfg(target_os = "linux")]
mod bwrap;

#[cfg(target_os = "linux")]
mod landlock;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
#[path = "unsupported.rs"]
mod platform;

pub const INNER_LANDLOCK_FLAG: &str = "--aegis-inner-landlock";

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("sandbox is required but unavailable on this system")]
    Required,

    #[error("sandbox is required but nested under an active outer sandbox")]
    RequiredNestedUnderOuterSandbox,

    #[error("sandbox setup failed: {0}")]
    SetupFailed(String),

    #[error("sandbox execution error: {0}")]
    Execution(String),

    #[error("failed to {operation} sandboxed command: {source}")]
    Spawn {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },

    #[error("sandbox I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteRoot {
    pub path: PathBuf,
    pub layer: CeilingLayer,
    pub location: String,
}

impl WriteRoot {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            layer: CeilingLayer::Computed,
            location: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SandboxConfig {
    pub allow_write: Vec<WriteRoot>,
    pub trusted_roots: Vec<WriteRoot>,
    pub allow_network: bool,
}

#[derive(Debug)]
pub struct PreparedSandboxCommand {
    pub command: std::process::Command,
    pub status: SandboxStatus,
    #[cfg(target_os = "linux")]
    reports_status: bool,
}

pub struct ReportedSandboxChild {
    child: Option<std::process::Child>,
    status: SandboxStatus,
    #[cfg(target_os = "linux")]
    release_fd: Option<i32>,
}

impl ReportedSandboxChild {
    pub fn from_child(child: std::process::Child, status: SandboxStatus) -> Self {
        Self {
            child: Some(child),
            status,
            #[cfg(target_os = "linux")]
            release_fd: None,
        }
    }

    pub fn status(&self) -> SandboxStatus {
        self.status
    }

    pub fn release(mut self) -> Result<std::process::Child, SandboxError> {
        #[cfg(target_os = "linux")]
        if let Some(fd) = self.release_fd.take() {
            let byte = b'1';
            // SAFETY: `fd` was taken from `self.release_fd` (open release pipe write end owned here) and `&byte` is valid for 1 readable byte.
            let n = unsafe { libc::write(fd, &byte as *const u8 as *const libc::c_void, 1) };
            // SAFETY: `fd` was taken from `self.release_fd`, so this is the sole close.
            unsafe { libc::close(fd) };
            if n != 1 {
                return Err(SandboxError::Io(std::io::Error::last_os_error()));
            }
        }
        self.child
            .take()
            .ok_or_else(|| SandboxError::Execution("sandbox child was already released".into()))
    }
}

impl Drop for ReportedSandboxChild {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        if let Some(fd) = self.release_fd.take() {
            // SAFETY: `fd` was taken from `self.release_fd`, so this is the sole close.
            unsafe { libc::close(fd) };
        }
    }
}

impl PreparedSandboxCommand {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn active(
        command: std::process::Command,
        #[cfg(target_os = "linux")] reports_status: bool,
    ) -> Self {
        Self {
            command,
            status: SandboxStatus::Active,
            #[cfg(target_os = "linux")]
            reports_status,
        }
    }

    #[cfg(unix)]
    pub fn exec(mut self) -> SandboxError {
        use std::os::unix::process::CommandExt;
        SandboxError::Io(self.command.exec())
    }

    #[cfg(target_os = "linux")]
    pub fn spawn_and_report(mut self) -> Result<ReportedSandboxChild, SandboxError> {
        if !self.reports_status {
            let child = self.command.spawn().map_err(SandboxError::Io)?;
            return Ok(ReportedSandboxChild {
                child: Some(child),
                status: self.status,
                release_fd: None,
            });
        }

        let mut fds = [0; 2];
        // SAFETY: `fds` is a valid 2-element `i32` array writable by `pipe2`.
        if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
            return Err(SandboxError::Io(std::io::Error::last_os_error()));
        }
        let (read_fd, write_fd) = (fds[0], fds[1]);
        let mut gate_fds = [0; 2];
        // SAFETY: `gate_fds` is a valid 2-element `i32` array writable by `pipe2`.
        if unsafe { libc::pipe2(gate_fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
            // SAFETY: `read_fd` is an open pipe descriptor owned here and is not used after close.
            unsafe { libc::close(read_fd) };
            // SAFETY: `write_fd` is an open pipe descriptor owned here and is not used after close.
            unsafe { libc::close(write_fd) };
            return Err(SandboxError::Io(std::io::Error::last_os_error()));
        }
        let (gate_read_fd, gate_write_fd) = (gate_fds[0], gate_fds[1]);

        self.command.env(landlock::AEGIS_LANDLOCK_STATUS_FD, "3");
        self.command.env(landlock::AEGIS_LANDLOCK_RELEASE_FD, "4");
        use std::os::unix::process::CommandExt;
        // SAFETY: the closure calls only async-signal-safe `dup2` and `close` on the open pipe fds created above.
        unsafe {
            self.command.pre_exec(move || {
                if libc::dup2(write_fd, 3) == -1 || libc::dup2(gate_read_fd, 4) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                if write_fd != 3 {
                    libc::close(write_fd);
                }
                if gate_read_fd != 4 {
                    libc::close(gate_read_fd);
                }
                Ok(())
            });
        }

        let child = self.command.spawn().map_err(SandboxError::Io)?;

        // SAFETY: `write_fd` and `gate_read_fd` are open pipe descriptors owned by the parent and are not used after close.
        unsafe {
            libc::close(write_fd);
            libc::close(gate_read_fd);
        }

        let status = read_status_from_fd(read_fd)?;
        // SAFETY: `read_fd` is an open pipe descriptor owned by the parent and is not used after close.
        unsafe {
            libc::close(read_fd);
        }

        Ok(ReportedSandboxChild {
            child: Some(child),
            status,
            release_fd: Some(gate_write_fd),
        })
    }

    #[cfg(not(target_os = "linux"))]
    pub fn spawn_and_report(mut self) -> Result<ReportedSandboxChild, SandboxError> {
        let child = self.command.spawn().map_err(SandboxError::Io)?;
        Ok(ReportedSandboxChild {
            child: Some(child),
            status: self.status,
        })
    }
}

#[cfg(target_os = "linux")]
fn read_status_from_fd(fd: i32) -> Result<SandboxStatus, SandboxError> {
    let mut buf = [0u8; 1];
    // SAFETY: `buf` is valid for 1 writable byte and `fd` is the open status pipe read end.
    let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, 1) };
    if n < 0 {
        return Err(SandboxError::Io(std::io::Error::last_os_error()));
    }
    if n == 0 {
        return Ok(SandboxStatus::NotAttempted);
    }
    match buf[0] {
        b'a' => Ok(SandboxStatus::Active),
        b'u' => Ok(SandboxStatus::Unavailable),
        b'n' => Ok(SandboxStatus::NotAttempted),
        b'c' => Ok(SandboxStatus::NotConfigured),
        _ => Err(SandboxError::Execution("unknown status byte".into())),
    }
}

pub fn sandbox_available_for(config: &SandboxConfig) -> bool {
    platform::sandbox_available_for(config)
}

pub fn wsl1_unavailable() -> bool {
    #[cfg(target_os = "linux")]
    {
        bwrap::wsl1_unavailable()
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

pub fn apparmor_userns_restricted() -> bool {
    #[cfg(target_os = "linux")]
    {
        bwrap::apparmor_userns_restricted()
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

pub fn prepare_for_exec(
    config: &SandboxConfig,
    program: &OsStr,
    args: &[OsString],
) -> Result<PreparedSandboxCommand, SandboxError> {
    platform::prepare_for_exec(config, program, args)
}

pub fn prepare_for_spawn(
    config: &SandboxConfig,
    program: &OsStr,
    args: &[OsString],
) -> Result<PreparedSandboxCommand, SandboxError> {
    platform::prepare_for_spawn(config, program, args)
}

#[cfg(target_os = "linux")]
pub fn run_inner_landlock_wrapper() -> i32 {
    landlock::run_inner_landlock_wrapper()
}

#[cfg(target_os = "linux")]
pub fn landlock_abi() -> u32 {
    landlock::landlock_abi()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_src_file_exceeds_800_lines() {
        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let entries = std::fs::read_dir(&src_dir)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", src_dir.display()));

        let mut offenders: Vec<(String, usize)> = Vec::new();
        for entry in entries {
            let entry = entry.unwrap_or_else(|e| panic!("failed to iterate src dir entry: {e}"));
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let contents = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
            let line_count = contents.lines().count();
            if line_count > 800 {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_owned())
                    .unwrap_or_else(|| format!("{}", path.display()));
                offenders.push((name, line_count));
            }
        }

        assert!(
            offenders.is_empty(),
            "aegis-sandbox source files exceed 800 LoC (acceptance gate): {offenders:?}"
        );
    }

    #[test]
    fn public_api_surface_survives_refactor() {
        let config = SandboxConfig::default();
        let err_display = SandboxError::Required.to_string();
        assert!(
            !err_display.is_empty(),
            "SandboxError::Required display is empty"
        );
        let _ = sandbox_available_for(&config);

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            use std::ffi::{OsStr, OsString};
            let program = OsStr::new("/usr/bin/true");
            let args: &[OsString] = &[];
            let _ = prepare_for_exec(&config, program, args);
        }
    }

    #[test]
    fn spawn_error_keeps_the_io_error_kind_as_source() {
        use std::error::Error;

        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let err = SandboxError::Spawn {
            operation: "spawn",
            source: io_err,
        };

        let source = err.source().expect("Spawn must carry an io::Error source");
        let io_source = source
            .downcast_ref::<std::io::Error>()
            .expect("source must be the original io::Error, not a stringified message");
        assert_eq!(io_source.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(err.to_string(), "failed to spawn sandboxed command: denied");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn read_status_from_fd_decodes_active_byte() {
        use std::io::Write;
        use std::os::unix::io::AsRawFd;

        let (reader, mut writer) = std::io::pipe().expect("pipe must succeed");
        writer.write_all(b"a").expect("write must succeed");
        drop(writer);
        assert_eq!(
            read_status_from_fd(reader.as_raw_fd()).expect("read must succeed"),
            SandboxStatus::Active
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn dropping_reported_sandbox_child_closes_release_fd() {
        use std::os::unix::io::IntoRawFd;

        let (_reader, writer) = std::io::pipe().expect("pipe must succeed");
        let raw_fd = writer.into_raw_fd();
        let child = std::process::Command::new("/bin/true")
            .spawn()
            .or_else(|_| std::process::Command::new("/usr/bin/true").spawn())
            .expect("true must spawn");
        let mut reported = ReportedSandboxChild {
            child: Some(child),
            status: SandboxStatus::Active,
            release_fd: Some(raw_fd),
        };
        if let Some(mut c) = reported.child.take() {
            c.wait().expect("true must exit");
        }
        drop(reported);

        // SAFETY: `fcntl(F_GETFD)` takes no pointers and returns `-1` (`EBADF`) once `raw_fd` is closed.
        let rc = unsafe { libc::fcntl(raw_fd, libc::F_GETFD) };
        assert_eq!(rc, -1, "Drop must close release_fd");
    }
}
