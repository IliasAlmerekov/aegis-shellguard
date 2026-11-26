#![cfg(target_os = "linux")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail loudly"
)]
#![expect(unsafe_code, reason = "FFI calls, each documented by a SAFETY comment")]
mod support;

use std::process::Command;

use tempfile::TempDir;

use support::aegis_bin;

fn encode_single_allow_write(path: &std::path::Path) -> String {
    let s = path.to_string_lossy();
    format!("{}:{}", s.len(), s)
}

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

#[test]
fn inner_landlock_wrapper_reports_active_status() {
    if aegis_sandbox::landlock_abi() < 1 {
        eprintln!("skipping: Landlock ABI < 1");
        return;
    }

    let workspace = TempDir::new().unwrap();

    let mut fds = [0; 2];
    assert_eq!(
        // SAFETY: `fds` is a valid 2-element `i32` array writable by `pipe`.
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

    // SAFETY: `write_fd` is an open pipe descriptor owned by this test and is not used after close.
    unsafe {
        libc::close(write_fd);
    }

    let mut buf = [0u8; 1];
    // SAFETY: `buf` is valid for 1 writable byte and `read_fd` is an open pipe descriptor.
    let n = unsafe { libc::read(read_fd, buf.as_mut_ptr() as *mut libc::c_void, 1) };
    // SAFETY: `read_fd` is an open pipe descriptor owned by this test and is not used after close.
    unsafe {
        libc::close(read_fd);
    }

    let status = child.wait().unwrap();

    assert_eq!(n, 1, "inner wrapper must report a status byte");
    assert_eq!(buf[0], b'a', "status must be Active");
    assert!(status.success(), "command must run");
}

#[test]
fn spawn_and_report_spawns_and_returns_active_status() {
    let config = aegis_sandbox::SandboxConfig {
        allow_write: vec![],
        ..Default::default()
    };
    if !support::confinement_available_for(&config) {
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

#[test]
fn sandboxed_command_runs_and_denies_write_outside_allow_write() {
    if aegis_sandbox::landlock_abi() < 1 {
        eprintln!("skipping: Landlock ABI < 1");
        return;
    }

    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();

    if !support::confinement_available_for(&aegis_sandbox::SandboxConfig {
        allow_write: vec![aegis_sandbox::WriteRoot::new(workspace.path())],
        trusted_roots: vec![aegis_sandbox::WriteRoot::new(workspace.path())],
        ..Default::default()
    }) {
        return;
    }

    support::write_global_config(
        home.path(),
        &format!(
            "[sandbox]\nallow_write = [\"{}\"]\n",
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
