#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail loudly"
)]
mod support;

use std::io::Write;
use std::process::Stdio;

use aegis_types::DegradationReason;
use tempfile::TempDir;

use support::base_command;

#[test]
fn router_degradations_use_neutral_wording_in_noninteractive_and_interactive_modes() {
    let cases = [
        ("$X -rf /", DegradationReason::DynamicSource),
        ("bash ./missing.sh", DegradationReason::UnsafeSource),
    ];

    for (cmd, reason) in cases {
        let home = TempDir::new().unwrap();
        let workspace = TempDir::new().unwrap();
        let expected_reason = format!(
            "Reason: Aegis could not resolve what this command runs: {}",
            reason.phrase()
        );

        let no_tty = base_command(home.path())
            .current_dir(workspace.path())
            .env("AEGIS_FORCE_NO_TTY", "1")
            .args(["-c", cmd])
            .output()
            .unwrap();
        assert_eq!(no_tty.status.code(), Some(2), "cmd={cmd}");
        let no_tty_stderr = String::from_utf8_lossy(&no_tty.stderr);
        assert!(
            no_tty_stderr.contains("non-interactive mode — one-time approval required"),
            "missing neutral header for {cmd}:\n{no_tty_stderr}"
        );
        assert!(
            no_tty_stderr.contains(&expected_reason),
            "missing {expected_reason:?} for {cmd}:\n{no_tty_stderr}"
        );
        assert!(
            !no_tty_stderr.to_ascii_lowercase().contains("language"),
            "non-interactive stderr for {cmd} must not mention language:\n{no_tty_stderr}"
        );

        let mut child = base_command(home.path())
            .current_dir(workspace.path())
            .env("AEGIS_FORCE_INTERACTIVE", "1")
            .args(["-c", cmd])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.as_mut().unwrap().write_all(b"n\n").unwrap();
        let interactive = child.wait_with_output().unwrap();
        assert_eq!(interactive.status.code(), Some(2), "cmd={cmd}");
        let interactive_stderr = String::from_utf8_lossy(&interactive.stderr);
        assert!(
            interactive_stderr.contains(&expected_reason),
            "missing {expected_reason:?} in interactive stderr for {cmd}:\n{interactive_stderr}"
        );
        assert!(
            interactive_stderr.contains("Approve this command once? [y/N]"),
            "missing Protect prompt for {cmd}:\n{interactive_stderr}"
        );
        assert!(
            !interactive_stderr.to_ascii_lowercase().contains("language"),
            "interactive stderr for {cmd} must not mention language:\n{interactive_stderr}"
        );
    }
}
