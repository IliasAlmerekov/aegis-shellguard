#![cfg(feature = "language-analysis")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail loudly"
)]

mod support;

use std::path::Path;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

fn evaluate(home: &Path, cwd: &Path, command: &str) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_aegis"))
        .args(["--output", "json", "-c", command])
        .env("HOME", home)
        .env("AEGIS_REAL_SHELL", "/bin/sh")
        .env("AEGIS_FORCE_NO_TTY", "1")
        .env("AEGIS_CI", "0")
        .current_dir(cwd)
        .output()
        .expect("aegis must run");
    serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "aegis must print a JSON evaluation ({err}): stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn assert_safe_script_shows_no_match(evaluation: &Value) {
    assert!(
        matches!(
            evaluation["decision"].as_str(),
            Some("auto_approve" | "prompt")
        ),
        "{evaluation:#}"
    );
    assert!(
        evaluation["matched_patterns"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "{evaluation:#}"
    );
}

#[test]
fn directly_executed_safe_shell_script_shows_no_match() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    support::write_executable(&cwd.path().join("probe.sh"), "#!/bin/sh\necho 1\n");

    let evaluation = evaluate(home.path(), cwd.path(), "./probe.sh");

    assert_safe_script_shows_no_match(&evaluation);
}

#[test]
fn safe_shell_script_run_through_sh_shows_no_match() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    support::write_executable(&cwd.path().join("probe.sh"), "#!/bin/sh\necho 1\n");

    let evaluation = evaluate(home.path(), cwd.path(), "sh ./probe.sh");

    assert_safe_script_shows_no_match(&evaluation);
}

#[test]
fn shell_script_hiding_a_risky_command_still_prompts() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    support::write_executable(
        &cwd.path().join("deploy.sh"),
        "#!/bin/sh\necho deploying\ngit push --force origin main\n",
    );

    let evaluation = evaluate(home.path(), cwd.path(), "./deploy.sh");

    assert_eq!(evaluation["decision"], "prompt", "{evaluation:#}");
    assert!(
        !evaluation.to_string().contains("origin main"),
        "the evaluation must not disclose script source: {evaluation:#}"
    );
}

#[test]
fn shell_script_running_another_script_is_not_claimed_safe() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    std::fs::write(cwd.path().join("helper.py"), "print(1)\n").unwrap();
    support::write_executable(
        &cwd.path().join("wrapper.sh"),
        "#!/bin/sh\npython3 ./helper.py\n",
    );

    let evaluation = evaluate(home.path(), cwd.path(), "./wrapper.sh");

    assert_eq!(evaluation["decision"], "prompt", "{evaluation:#}");
}

#[test]
fn shell_script_executing_another_script_by_path_is_not_claimed_safe() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    support::write_executable(
        &cwd.path().join("inner.sh"),
        "#!/bin/sh\ngit push --force origin main\n",
    );
    support::write_executable(&cwd.path().join("outer.sh"), "#!/bin/sh\n./inner.sh\n");

    let evaluation = evaluate(home.path(), cwd.path(), "./outer.sh");

    assert_eq!(evaluation["decision"], "prompt", "{evaluation:#}");
}
