//! Verify that a re-executed test reached the end of its child body.

use std::process::{Command, Output};

const MARKER: &str = "AEGIS_ISOLATED_TEST_COMPLETED";

/// Acknowledges successful completion of a child test body.
pub struct Completion;

impl Drop for Completion {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            println!("{MARKER}");
        }
    }
}

/// Prepare an exact re-execution of the current test.
pub fn command() -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args([
        "--exact",
        std::thread::current().name().unwrap(),
        "--nocapture",
    ]);
    command
}

/// Check the exit status and child-body acknowledgement.
pub fn completed(output: &Output) -> bool {
    output.status.success()
        && output
            .stdout
            .split(|byte| *byte == b'\n')
            .any(|line| line.ends_with(MARKER.as_bytes()))
}

/// Run a child test and report its output on failure.
pub fn run(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        completed(&output),
        "isolated test failed or did not complete\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
