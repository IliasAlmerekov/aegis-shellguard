//! Shell/Bash script analysis through the real CLI (issue #383, plan
//! Iteration 8).
//!
//! A shell script file must get at least the treatment a Python script file
//! gets: its contents are analyzed, a script whose commands are all safe
//! shows no Match, and a script that hides a risky command still prompts.
//!
//! The CLI clamps the analysis deadline to 100 ms, and a loaded test run can
//! miss it (#458). These tests assert only what holds either way. The Match
//! and completion checks live in `analysis_orchestrate::cli_deadline_parity`.

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

/// A safe script auto-approves when analysis completes and prompts when the
/// worker misses its deadline (#458). It shows no Match either way.
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

    // The GIT-003 Match needs completed analysis, so it is checked in
    // `analysis_orchestrate::cli_deadline_parity` (#458).
    assert_eq!(evaluation["decision"], "prompt", "{evaluation:#}");
    // ADR-022 §10: script contents never leave the analysis stage. This
    // passes even when analysis degraded, so the check that runs against a
    // real Match is in `analysis_orchestrate::cli_deadline_parity`.
    assert!(
        !evaluation.to_string().contains("origin main"),
        "the evaluation must not disclose script source: {evaluation:#}"
    );
}

#[test]
fn shell_script_running_another_script_is_not_claimed_safe() {
    // The nested `python3 ./helper.py` payload is a file this pass does not
    // open, so the script's effect is unknown and must not auto-approve.
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
    // `./inner.sh` runs a file whose contents are not in the outer script, so
    // the outer script cannot be approved on what it shows.
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
