//! Shell/Bash script analysis through the real CLI (issue #383, plan
//! Iteration 8).
//!
//! A shell script file must get at least the treatment a Python script file
//! gets: its contents are analyzed, a script whose commands are all safe
//! normally runs without a prompt, and a script that hides a risky command
//! still prompts. (#458: the CLI can't force a generous analysis budget, so
//! the "safe script" tests below tolerate the rare degraded-prompt case —
//! see `analysis_orchestrate::cli_deadline_parity` for the strict version.)

mod support;

use std::path::Path;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

fn evaluate(home: &Path, cwd: &Path, command: &str) -> Value {
    // #458: `language_analysis.timeout_ms` clamps to 100ms at every config
    // layer (ADR-022 §6/§7) — setting it here from a test would be a no-op,
    // so this CLI evaluation is exercised at whatever the clamped ceiling
    // allows. See the module doc above for how the "safe script" assertions
    // account for that.
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

// #458: the clamped 100ms `language_analysis.timeout_ms` ceiling means a
// loaded `cargo test --workspace` run can miss it and degrade instead of
// completing — turning a genuinely safe script's "auto_approve" into a
// "prompt" for a reason unrelated to the script's content. `matched_patterns`
// stays empty either way (Degraded records no synthetic Match), so that part
// keeps proving the script isn't flagged; the stronger "analysis actually
// completed" claim moved to
// `analysis_orchestrate::cli_deadline_parity::safe_shell_script_completes_with_no_match_in_both_invocation_shapes`.
fn assert_safe_script_is_approved_or_degrades_to_prompt(evaluation: &Value) {
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
        "a safe script must show no risky Match either way: {evaluation:#}"
    );
}

#[test]
fn directly_executed_safe_shell_script_is_auto_approved() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    support::write_executable(&cwd.path().join("probe.sh"), "#!/bin/sh\necho 1\n");

    let evaluation = evaluate(home.path(), cwd.path(), "./probe.sh");

    assert_safe_script_is_approved_or_degrades_to_prompt(&evaluation);
}

#[test]
fn shell_script_run_through_sh_is_auto_approved_when_safe() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    support::write_executable(&cwd.path().join("probe.sh"), "#!/bin/sh\necho 1\n");

    let evaluation = evaluate(home.path(), cwd.path(), "sh ./probe.sh");

    assert_safe_script_is_approved_or_degrades_to_prompt(&evaluation);
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

    // "prompt" holds whether analysis completes with the GIT-003 Match or
    // degrades on the clamped 100ms deadline under a loaded
    // `cargo test --workspace` run (#458) — a hidden force push must never
    // silently auto-approve either way. The GIT-003-Match and
    // no-source-disclosure claims, which only hold once analysis Completes,
    // moved to
    // `analysis_orchestrate::cli_deadline_parity::shell_script_hiding_a_force_push_yields_git_003_without_disclosing_source`.
    assert_eq!(evaluation["decision"], "prompt", "{evaluation:#}");
    // ADR-022 §10: script contents never leave the analysis stage, whether
    // or not a Match fired.
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
