//! Regression tests for issue #211: Landlock must be applied in the innermost
//! process (inside bwrap's mount namespace) so the bwrap + Landlock layers
//! compose, and `PR_SET_NO_NEW_PRIVS` must be set before
//! `landlock_restrict_self`.
//!
//! Both tests are gated on a Landlock-capable host (ABI >= 1) and skip
//! otherwise, so they stay green on kernels without Landlock.

mod support;

use std::process::Command;

use tempfile::TempDir;

use support::aegis_bin;

/// Serialize a single `allow_write` path in the same length-prefixed format the
/// sandbox crate uses (`<len>:<bytes>`), so the inner wrapper can parse it.
fn encode_single_allow_write(path: &std::path::Path) -> String {
    let s = path.to_string_lossy();
    format!("{}:{}", s.len(), s)
}

/// The inner landlock wrapper must return the internal-error exit code (4),
/// not a command exit code, when it cannot set up Landlock (CONVENTION.md §3:
/// codes 1–N are the executed command's code; internal Aegis errors are 4).
#[test]
fn inner_landlock_wrapper_returns_internal_error_code_on_missing_config() {
    let output = Command::new(aegis_bin())
        .args(["--aegis-inner-landlock", "/bin/sh", "-c", "exit 0"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(4),
        "missing AEGIS_LANDLOCK_ALLOW_WRITE must yield the internal-error exit code (4), \
         not a command exit code; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The inner landlock wrapper must report the actual status (Active) over the
/// status fd after applying Landlock, so the parent can audit what actually
/// applied at the execution seam rather than what preparation predicted.
#[test]
fn inner_landlock_wrapper_reports_active_status() {
    if aegis_sandbox::landlock_abi() < 1 {
        eprintln!("skipping: Landlock ABI < 1");
        return;
    }

    let workspace = TempDir::new().unwrap();

    let mut fds = [0; 2];
    assert_eq!(
        unsafe { libc::pipe(fds.as_mut_ptr()) },
        0,
        "pipe must succeed"
    );
    let (read_fd, write_fd) = (fds[0], fds[1]);

    let mut child = Command::new(aegis_bin())
        .env(
            "AEGIS_LANDLOCK_ALLOW_WRITE",
            encode_single_allow_write(workspace.path()),
        )
        .env("AEGIS_LANDLOCK_STATUS_FD", write_fd.to_string())
        .args(["--aegis-inner-landlock", "/bin/sh", "-c", "exit 0"])
        .spawn()
        .unwrap();

    // Close the parent's copy of the write end so the read returns EOF once the
    // inner wrapper closes it.
    unsafe {
        libc::close(write_fd);
    }

    // Read the status byte.
    let mut buf = [0u8; 1];
    let n = unsafe { libc::read(read_fd, buf.as_mut_ptr() as *mut libc::c_void, 1) };
    unsafe {
        libc::close(read_fd);
    }

    let status = child.wait().unwrap();

    assert_eq!(n, 1, "inner wrapper must report a status byte");
    assert_eq!(buf[0], b'a', "status must be Active");
    assert!(status.success(), "command must run");
}

/// `spawn_and_report` must spawn the prepared bwrap command and return the
/// actual status. The spawn-safe path has no inner wrapper, so it returns the
/// preparation status (Active). The exec path's wrapper gate is exercised
/// end-to-end through the aegis binary in the shell-flow tests.
#[test]
fn spawn_and_report_spawns_and_returns_active_status() {
    let config = aegis_sandbox::SandboxConfig {
        allow_write: vec![],
        ..Default::default()
    };
    if !aegis_sandbox::sandbox_available_for(&config) {
        eprintln!("skipping: no usable confinement backend on this host");
        return;
    }
    let prepared = aegis_sandbox::prepare_for_spawn(
        &config,
        std::ffi::OsStr::new("/bin/sh"),
        &[
            std::ffi::OsString::from("-c"),
            std::ffi::OsString::from("exit 0"),
        ],
    )
    .expect("prepare_for_exec must succeed");

    let reported = prepared
        .spawn_and_report()
        .expect("spawn_and_report must succeed");
    let status = reported.status();
    let child = reported.release().expect("child must be released");
    let output = child.wait_with_output().unwrap();

    assert_eq!(status, aegis_types::SandboxStatus::Active);
    assert!(output.status.success(), "command must run");
}

/// The inner landlock wrapper, invoked directly (no bwrap), must apply Landlock
/// and deny a write outside `allow_write`. This isolates the Landlock layer
/// from bwrap's namespace isolation.
#[test]
fn inner_landlock_wrapper_denies_write_outside_allow_write() {
    if aegis_sandbox::landlock_abi() < 1 {
        eprintln!("skipping: Landlock ABI < 1");
        return;
    }

    let workspace = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();

    let command = format!(
        "echo ok > {}/inside.txt; echo bad > {}/outside.txt",
        workspace.path().display(),
        outside.path().display()
    );

    let output = Command::new(aegis_bin())
        .env(
            "AEGIS_LANDLOCK_ALLOW_WRITE",
            encode_single_allow_write(workspace.path()),
        )
        .args(["--aegis-inner-landlock", "/bin/sh", "-c", &command])
        .output()
        .unwrap();

    assert!(
        workspace.path().join("inside.txt").exists(),
        "write inside allow_write must succeed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !outside.path().join("outside.txt").exists(),
        "write outside allow_write must be denied by Landlock; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// End-to-end: a real command with `sandbox.enabled = true` and a non-empty
/// `allow_write` must execute inside bwrap **and** a write outside
/// `allow_write` must be denied. This is the acceptance criterion for #211.
#[test]
fn sandboxed_command_runs_and_denies_write_outside_allow_write() {
    if aegis_sandbox::landlock_abi() < 1 {
        eprintln!("skipping: Landlock ABI < 1");
        return;
    }

    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();

    // bwrap must actually work here: this case asserts the composed layers, so a
    // host without a usable confinement backend would fail on the environment,
    // not on the regression. ADR-029 makes the backend mandatory, and once CI
    // installs bubblewrap this skip stops firing there.
    if !aegis_sandbox::sandbox_available_for(&aegis_sandbox::SandboxConfig {
        allow_write: vec![workspace.path().to_path_buf()],
        ..Default::default()
    }) {
        eprintln!("skipping: no usable confinement backend on this host");
        return;
    }

    // The C3 ratchet keeps the intersection of base and project `allow_write`,
    // so a non-empty `allow_write` must be set in the trusted global layer
    // (the project layer can only tighten to a subset of it).
    support::write_global_config(
        home.path(),
        &format!(
            "[sandbox]\nenabled = true\nallow_write = [\"{}\"]\n",
            workspace.path().display()
        ),
    );

    let command = format!(
        "echo ok > {}/inside.txt; echo bad > {}/outside.txt",
        workspace.path().display(),
        outside.path().display()
    );

    let output = Command::new(aegis_bin())
        .env("AEGIS_REAL_SHELL", "/bin/sh")
        .env("AEGIS_CI", "0")
        .env("HOME", home.path())
        .current_dir(workspace.path())
        .args(["-c", &command])
        .output()
        .unwrap();

    assert!(
        workspace.path().join("inside.txt").exists(),
        "sandboxed command must execute and write inside allow_write; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !outside.path().join("outside.txt").exists(),
        "write outside allow_write must be denied; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
