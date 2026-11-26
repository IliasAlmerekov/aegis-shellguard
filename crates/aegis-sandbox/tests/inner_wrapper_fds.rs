#[cfg(not(target_os = "linux"))]
fn main() {}

#[cfg(target_os = "linux")]
fn main() {
    const CHILD: &str = "AEGIS_INNER_WRAPPER_FDS_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let code = aegis_sandbox::run_inner_landlock_wrapper();
        assert_eq!(
            code, 4,
            "EOF on release fd must fail closed (EXIT_INTERNAL)"
        );
        // SAFETY: fcntl with F_GETFD takes no pointer and reports whether the descriptor is closed.
        let status_rc = unsafe { libc::fcntl(1, libc::F_GETFD) };
        assert_eq!(status_rc, -1, "report_status must close status_fd");
        // SAFETY: fcntl with F_GETFD takes no pointer and reports whether the descriptor is closed.
        let release_rc = unsafe { libc::fcntl(0, libc::F_GETFD) };
        assert_eq!(release_rc, -1, "wait_for_release must close release_fd");
    } else {
        let output = std::process::Command::new(
            std::env::current_exe().expect("test executable must exist"),
        )
        .env(CHILD, "1")
        .env("AEGIS_LANDLOCK_ALLOW_WRITE", "")
        .env("AEGIS_LANDLOCK_STATUS_FD", "1")
        .env("AEGIS_LANDLOCK_RELEASE_FD", "0")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .output()
        .expect("wrapper child must run");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            output.stdout, b"a",
            "report_status must write Active ('a') and close the status fd"
        );
    }
}
