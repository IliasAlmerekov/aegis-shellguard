//! Integration test for the inner landlock wrapper's status and release fds.
//!
//! This file is its own test binary and holds one test, so editing the process
//! environment cannot race another test.

#![cfg(target_os = "linux")]

use std::io::Read;
use std::os::unix::io::IntoRawFd;

#[test]
fn inner_landlock_wrapper_reports_status_and_closes_release_fd() {
    let (mut status_reader, status_writer) = std::io::pipe().expect("status pipe must succeed");
    let (release_reader, release_writer) = std::io::pipe().expect("release pipe must succeed");
    // Drop the release write end so wait_for_release reads EOF, closes its fd,
    // and returns before execing anything from argv.
    drop(release_writer);

    let status_fd = status_writer.into_raw_fd();
    let release_fd = release_reader.into_raw_fd();

    // SAFETY: this integration-test binary runs only this test, so no other
    // thread reads or mutates the environment.
    unsafe {
        std::env::set_var("AEGIS_LANDLOCK_ALLOW_WRITE", "");
        std::env::set_var("AEGIS_LANDLOCK_STATUS_FD", status_fd.to_string());
        std::env::set_var("AEGIS_LANDLOCK_RELEASE_FD", release_fd.to_string());
    }

    let code = aegis_sandbox::run_inner_landlock_wrapper();

    // SAFETY: this integration-test binary runs only this test, so no other
    // thread reads or mutates the environment.
    unsafe {
        std::env::remove_var("AEGIS_LANDLOCK_ALLOW_WRITE");
        std::env::remove_var("AEGIS_LANDLOCK_STATUS_FD");
        std::env::remove_var("AEGIS_LANDLOCK_RELEASE_FD");
    }

    assert_eq!(
        code, 4,
        "EOF on release fd must fail closed (EXIT_INTERNAL)"
    );

    // SAFETY: `fcntl(F_GETFD)` takes no pointers and returns `-1` (`EBADF`) once `status_fd` is closed.
    let status_rc = unsafe { libc::fcntl(status_fd, libc::F_GETFD) };
    assert_eq!(status_rc, -1, "report_status must close status_fd");

    let mut buf = Vec::new();
    status_reader
        .read_to_end(&mut buf)
        .expect("reading status pipe must succeed");
    assert_eq!(
        buf, b"a",
        "report_status must write Active ('a') and close the status fd"
    );

    // SAFETY: `fcntl(F_GETFD)` takes no pointers and returns `-1` (`EBADF`) once `release_fd` is closed.
    let rc = unsafe { libc::fcntl(release_fd, libc::F_GETFD) };
    assert_eq!(rc, -1, "wait_for_release must close release_fd");
}
