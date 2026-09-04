//! Sandboxing layer for Aegis.
//!
//! Provides typed, presentation-free preparation through
//! [`PreparedSandboxCommand`] plus the legacy [`SandboxExecutor`] interface on
//! supported platforms:
//! - **Linux**: bwrap + Landlock
//! - **macOS**: Seatbelt (`sandbox-exec`)
//!
//! Native Windows is intentionally unsupported for Aegis 1.0. Windows users
//! should run Aegis inside WSL2, where it behaves as Linux.
//!
//! Platform-specific implementation lives in a private `platform` module alias
//! that resolves to `linux.rs`, `macos.rs`, or `unsupported.rs` depending on the
//! build target. Shared test support lives in `support.rs`.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use aegis_types::SandboxStatus;

mod support;

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

// Native `windows` is intentionally routed to the unsupported module for
// Aegis 1.0; Windows users should run Aegis inside WSL2/Linux.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
#[path = "unsupported.rs"]
mod platform;

/// Internal flag that re-execs the Aegis binary as a thin landlock wrapper
/// inside bwrap's mount namespace. Owned here so `main.rs` and the sandbox
/// crate cannot drift.
pub const INNER_LANDLOCK_FLAG: &str = "--aegis-inner-landlock";

/// Exit code for internal Aegis errors (CONVENTION.md §3): codes 1–N are the
/// executed command's exit code; internal errors are 4. The inner landlock
/// wrapper returns this when it cannot set up Landlock, so a failure is
/// distinguishable from a command that ran and exited 1.
pub const EXIT_INTERNAL: i32 = 4;

/// Typed error for sandbox operations.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    /// The sandbox was marked `required = true` but is unavailable on this system.
    #[error("sandbox is required but unavailable on this system")]
    Required,

    /// The sandbox was marked `required = true` but is unavailable because this
    /// process is already confined by an active outer Seatbelt profile, which
    /// refuses the nested `sandbox_apply` call Aegis needs to make (macOS only).
    /// Kept distinct from [`Self::Required`] because, unlike a missing or broken
    /// `sandbox-exec`, this cause has a practical remedy to name.
    #[error("sandbox is required but nested under an active outer sandbox")]
    RequiredNestedUnderOuterSandbox,

    /// bwrap failed to set up the sandbox (namespace, mount, or permissions error).
    #[error("sandbox setup failed: {0}")]
    SetupFailed(String),

    /// A sandbox execution error occurred (e.g. failed to spawn bwrap).
    #[error("sandbox execution error: {0}")]
    Execution(String),

    /// Wrapped I/O error.
    #[error("sandbox I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Configuration for the sandbox layer.
#[derive(Debug, Clone, Default)]
pub struct SandboxConfig {
    /// Paths the sandboxed process is allowed to write to.
    pub allow_write: Vec<PathBuf>,
    /// Whether the sandboxed process is allowed to access the network.
    pub allow_network: bool,
    /// If `true`, failure to set up the sandbox is a hard error rather than a
    /// graceful fallback.
    pub required: bool,
}

/// A compiled sandbox profile derived from a [`SandboxConfig`].
#[derive(Debug, Clone)]
pub struct SandboxProfile {
    config: SandboxConfig,
}

/// Executes a command inside the sandbox described by a [`SandboxProfile`].
pub struct SandboxExecutor {
    profile: SandboxProfile,
}

/// Outcome of a sandboxed command execution.
#[derive(Debug)]
pub enum SandboxResult {
    /// The command ran successfully; the inner value is its exit code.
    Success(i32),
    /// The sandbox was unavailable and was skipped because `required` was `false`.
    Unavailable,
}

/// A command prepared for the selected confinement path.
///
/// `status` describes the command stored in `command`. Preparation never
/// renders user-facing output or applies process-wide restrictions.
#[derive(Debug)]
pub struct PreparedSandboxCommand {
    /// The confined or direct command that the caller may execute or spawn.
    pub command: std::process::Command,
    /// Factual Sandbox status for the prepared command.
    pub status: SandboxStatus,
    /// Whether the inner landlock wrapper will report the actual status over a
    /// pipe when spawned (true on the exec path with a non-empty `allow_write`).
    #[cfg(target_os = "linux")]
    reports_status: bool,
}

/// A sandbox child that has reported its actual confinement status.
///
/// On Linux with the inner Landlock wrapper, the wrapped program remains
/// stopped until [`Self::release`] is called. Dropping this value closes the
/// gate, causing the wrapper to fail closed before it can exec the program.
pub struct ReportedSandboxChild {
    child: Option<std::process::Child>,
    status: SandboxStatus,
    #[cfg(target_os = "linux")]
    release_fd: Option<i32>,
}

impl ReportedSandboxChild {
    /// Wrap an already-started child whose status needs no release gate.
    pub fn from_child(child: std::process::Child, status: SandboxStatus) -> Self {
        Self {
            child: Some(child),
            status,
            #[cfg(target_os = "linux")]
            release_fd: None,
        }
    }

    /// Return the actual status the launch path reported.
    pub fn status(&self) -> SandboxStatus {
        self.status
    }

    /// Permit the inner wrapper to exec the wrapped program and return its child.
    pub fn release(mut self) -> Result<std::process::Child, SandboxError> {
        #[cfg(target_os = "linux")]
        if let Some(fd) = self.release_fd.take() {
            let byte = b'1';
            let n = unsafe { libc::write(fd, &byte as *const u8 as *const libc::c_void, 1) };
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

    fn unavailable(command: std::process::Command) -> Self {
        Self {
            command,
            status: SandboxStatus::Unavailable,
            #[cfg(target_os = "linux")]
            reports_status: false,
        }
    }

    /// Replace the current process with the prepared command.
    ///
    /// On Linux the Landlock layer is applied by the innermost re-exec'd
    /// wrapper inside bwrap's mount namespace, so this method never restricts
    /// the current process. Watch callers may therefore use it safely.
    ///
    /// This method does not return when process replacement succeeds. It
    /// returns [`SandboxError::Io`] if the operating-system `exec` call fails.
    #[cfg(unix)]
    pub fn exec(mut self) -> SandboxError {
        use std::os::unix::process::CommandExt;
        SandboxError::Io(self.command.exec())
    }

    /// Spawn the prepared command and return the actual status reported by the
    /// inner landlock wrapper, so the caller can audit what actually applied at
    /// the execution seam rather than what preparation predicted.
    ///
    /// When the landlock wrapper is present, a pipe is set up and the wrapper
    /// writes its actual status over it; an empty read (EOF) means the wrapper
    /// failed closed and the command never ran (`NotAttempted`). When the
    /// wrapper is absent, the command is spawned directly and the preparation
    /// status is returned.
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

        // Set up the status-report pipe.
        let mut fds = [0; 2];
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return Err(SandboxError::Io(std::io::Error::last_os_error()));
        }
        let (read_fd, write_fd) = (fds[0], fds[1]);
        let mut gate_fds = [0; 2];
        if unsafe { libc::pipe(gate_fds.as_mut_ptr()) } != 0 {
            unsafe { libc::close(read_fd) };
            unsafe { libc::close(write_fd) };
            return Err(SandboxError::Io(std::io::Error::last_os_error()));
        }
        let (gate_read_fd, gate_write_fd) = (gate_fds[0], gate_fds[1]);

        // Set the write end as fd 3 on the command, and tell the inner wrapper
        // which fd to report over.
        self.command.env(landlock::AEGIS_LANDLOCK_STATUS_FD, "3");
        self.command.env(landlock::AEGIS_LANDLOCK_RELEASE_FD, "4");
        use std::os::unix::process::CommandExt;
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

        // Spawn the command.
        let child = self.command.spawn().map_err(SandboxError::Io)?;

        // Close the parent's copy of the write end so the read returns EOF once
        // the inner wrapper closes it.
        unsafe {
            libc::close(write_fd);
            libc::close(gate_read_fd);
        }

        // Read the actual status.
        let status = read_status_from_fd(read_fd)?;
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

/// Read the status byte the inner landlock wrapper wrote to `fd`. An empty read
/// (EOF) means the wrapper failed closed and the command never ran.
#[cfg(target_os = "linux")]
fn read_status_from_fd(fd: i32) -> Result<SandboxStatus, SandboxError> {
    let mut buf = [0u8; 1];
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

// ── Public availability query ─────────────────────────────────────────────────

/// Return `true` when a diagnostic availability probe succeeds for `config`.
///
/// This probe is not authoritative for execution or Audit; callers must use
/// the status returned by [`prepare_for_exec`] or [`prepare_for_spawn`]. Native
/// Windows and other unsupported targets always return `false`.
pub fn sandbox_available_for(config: &SandboxConfig) -> bool {
    platform::sandbox_available_for(config)
}

/// Whether this host is WSL1, which cannot create the user namespaces the Linux
/// sandbox needs. Only meaningful on Linux; other targets return `false`.
///
/// Callers use this to name the WSL1 cause in the startup warning instead of
/// the generic unavailability message (ADR-029 §3).
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

// ── Implementation ────────────────────────────────────────────────────────────

impl SandboxProfile {
    pub fn from_config(config: &SandboxConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }
}

impl SandboxExecutor {
    pub fn new(profile: SandboxProfile) -> Self {
        Self { profile }
    }

    pub fn run(&self, cmd: &str) -> Result<SandboxResult, SandboxError> {
        platform::run(&self.profile.config, cmd)
    }
}

/// Prepare a [`std::process::Command`] suitable for POSIX `exec()` that wraps
/// `program` and `args` inside the sandbox described by `config`.
///
/// On Linux, Landlock is deferred until [`PreparedSandboxCommand::exec`] so
/// preparation cannot restrict the caller before its Audit append. When
/// unavailable and `required` is `false`, returns a direct command with
/// `SandboxStatus::Unavailable`. Required unavailability returns
/// `Err(SandboxError::Required)`.
///
/// Returns [`SandboxError::Required`] when required infrastructure is
/// unavailable, [`SandboxError::Execution`] when configured paths or profile
/// construction fail, and [`SandboxError::SetupFailed`] when a platform
/// launcher rejects the prepared profile.
pub fn prepare_for_exec(
    config: &SandboxConfig,
    program: &OsStr,
    args: &[OsString],
) -> Result<PreparedSandboxCommand, SandboxError> {
    platform::prepare_for_exec(config, program, args)
}

/// Prepare a child command without applying process-wide restrictions.
///
/// This is the spawn-safe entry point for persistent callers such as Watch.
/// Returns [`SandboxError::Required`] when required infrastructure is
/// unavailable, [`SandboxError::Execution`] when configured paths or profile
/// construction fail, and [`SandboxError::SetupFailed`] when a platform
/// launcher rejects the prepared profile.
pub fn prepare_for_spawn(
    config: &SandboxConfig,
    program: &OsStr,
    args: &[OsString],
) -> Result<PreparedSandboxCommand, SandboxError> {
    platform::prepare_for_spawn(config, program, args)
}

/// Run the inner landlock wrapper: read the serialized `allow_write` config
/// from the environment, apply Landlock, then exec the real program. Returns a
/// process exit code; never returns on success (exec replaces the process).
///
/// This is the re-exec'd innermost process inside bwrap's mount namespace. It
/// must stay thin: no config-from-disk, no audit, no snapshots. Fail-closed:
/// if Landlock cannot be applied, it exits non-zero before exec'ing the target.
#[cfg(target_os = "linux")]
pub fn run_inner_landlock_wrapper() -> i32 {
    landlock::run_inner_landlock_wrapper()
}

/// Return the Landlock ABI version supported by this kernel (0 if unavailable).
#[cfg(target_os = "linux")]
pub fn landlock_abi() -> u32 {
    landlock::landlock_abi()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Refactor acceptance guards (file-size split) ──────────────────────────

    /// Size guard for the split-aegis-sandbox refactor.
    ///
    /// Acceptance criterion from the plan: "No `crates/aegis-sandbox/src/*.rs`
    /// file exceeds 800 LoC." This test scans every `*.rs` direct child of
    /// `src/` at runtime and asserts each is at most 800 lines (counting all
    /// lines, matching `wc -l`). It MUST FAIL now because `lib.rs` is 2071
    /// LoC, and MUST PASS after the refactor splits the code into focused
    /// platform modules.
    #[test]
    fn no_src_file_exceeds_800_lines() {
        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let entries = std::fs::read_dir(&src_dir)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", src_dir.display()));

        let mut offenders: Vec<(String, usize)> = Vec::new();
        for entry in entries {
            let entry = entry.unwrap_or_else(|e| panic!("failed to iterate src dir entry: {e}"));
            let path = entry.path();
            // Only direct children of src/ that end in .rs.
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

    /// Public API presence guard for the split-aegis-sandbox refactor.
    ///
    /// The refactor must preserve every public item listed in the plan's
    /// "No public API changes for" acceptance criterion. Constructing/valuing
    /// each type and calling each function here anchors them at compile time
    /// and runtime; if the green-tester accidentally removes or renames one,
    /// this test fails to compile or fails at runtime.
    #[test]
    fn public_api_surface_survives_refactor() {
        // SandboxConfig
        let config = SandboxConfig::default();
        // SandboxProfile
        let profile = SandboxProfile::from_config(&config);
        // SandboxExecutor (construct only — do not call run() to avoid forking)
        let _executor = SandboxExecutor::new(profile);
        // SandboxResult
        let result: SandboxResult = SandboxResult::Unavailable;
        assert!(matches!(result, SandboxResult::Unavailable));
        // SandboxError
        let err_display = SandboxError::Required.to_string();
        assert!(
            !err_display.is_empty(),
            "SandboxError::Required display is empty"
        );
        // sandbox_available_for
        let _ = sandbox_available_for(&config);

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            use std::ffi::{OsStr, OsString};
            let program = OsStr::new("/usr/bin/true");
            let args: &[OsString] = &[];
            // POSIX prepare_for_exec — anchor its signature; ignore the outcome
            // (it may error or succeed depending on environment, which is fine).
            let _ = prepare_for_exec(&config, program, args);
        }
    }
}
