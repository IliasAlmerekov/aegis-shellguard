//! M3a parity contract (`TASKS.md#M3a`): the `Session-start notice` an agent
//! hook emits must report the same `Effective enforcement state` that
//! `aegis status` reports for the same environment.
//!
//! `aegis status` is the authoritative surface for effective state. The hooks
//! deliberately resolve the Toggle and the CI override inline in shell instead
//! of sourcing the managed helper (ADR-007), so no compiler or linker keeps
//! them agreeing with `runtime_gate::is_ci_environment` and
//! `toggle::disabled_flag_path`. These tests are that guard.
//!
//! Each case derives the state twice from independent implementations — once by
//! parsing `aegis status` stdout, once by classifying the notice text — and
//! requires the two to agree. Neither answer is computed from the other, so a
//! drift in either implementation fails the test rather than cancelling out.

mod support;

use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

// `CI_MARKER_VARS` is the same list `run_script_with_env` clears for the hook,
// shared rather than restated: if the two lists drifted apart, the two surfaces
// would stop running under identical environments — the exact property these
// cases exist to compare.
use support::agent_hooks::{CI_MARKER_VARS, aegis_test_binary, run_script_with_env};

/// The session-start hooks under this contract, by script path.
const SESSION_START_HOOKS: [&str; 2] = [
    "hooks/claude-session-start.sh",
    "hooks/codex-session-start.sh",
];

/// The effective enforcement state a surface reports: whether commands are
/// actually guarded, and whether a CI override is what makes them guarded.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum EffectiveState {
    /// Aegis inspects commands; the Toggle is on.
    Enforcing,
    /// Aegis passes commands through unguarded; the Toggle is off.
    DisabledPassthrough,
    /// Aegis inspects commands even though the Toggle is off, because CI
    /// overrides it.
    CiOverride,
}

fn write_disabled_flag(home: &Path) {
    fs::create_dir_all(home.join(".aegis")).unwrap();
    fs::write(home.join(".aegis").join("disabled"), "timestamp=x\npid=1\n").unwrap();
}

/// Parse the `effective mode:` line `aegis status` prints.
fn status_effective_state(home: &Path, cwd: &Path, envs: &[(&str, &str)]) -> EffectiveState {
    let mut command = Command::new(aegis_test_binary());
    command.arg("status").env("HOME", home).current_dir(cwd);
    for key in CI_MARKER_VARS {
        command.env_remove(key);
    }
    for (key, value) in envs {
        command.env(key, value);
    }

    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "aegis status must succeed: stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let mode = stdout
        .lines()
        .find_map(|line| line.strip_prefix("effective mode: "))
        .unwrap_or_else(|| panic!("aegis status must print an effective mode line:\n{stdout}"));

    match mode.trim() {
        "enforcing (CI override)" => EffectiveState::CiOverride,
        "disabled passthrough" => EffectiveState::DisabledPassthrough,
        "enforcing" => EffectiveState::Enforcing,
        other => panic!("unrecognized effective mode in aegis status: {other:?}"),
    }
}

/// Classify the notice a session-start hook emits, by the sentence that opens
/// it. The three branches are mutually exclusive by construction: only the
/// override notice names both enforcement and the overridden Toggle.
fn notice_effective_state(home: &Path, hook: &str, envs: &[(&str, &str)]) -> EffectiveState {
    let output = run_script_with_env(hook, home, &[], None, envs);
    assert!(
        output.status.success(),
        "{hook} must exit zero: stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{hook} must not write stderr: stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "{hook} must emit protocol-valid JSON ({err}): stdout=\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    let notice = json["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap_or_else(|| panic!("{hook} must emit additionalContext as a string"));

    if notice.contains("Aegis is disabled: commands run in unguarded passthrough.") {
        EffectiveState::DisabledPassthrough
    } else if notice.contains("Aegis is enforced: the local disabled Toggle is overridden by CI.") {
        EffectiveState::CiOverride
    } else if notice.contains("All Bash tool commands must be routed through aegis.") {
        EffectiveState::Enforcing
    } else {
        panic!("{hook} emitted an unclassifiable session-start notice: {notice:?}")
    }
}

/// Assert both session-start hooks agree with `aegis status` under one
/// environment. `disabled` writes the Toggle flag before either surface runs.
fn assert_parity(disabled: bool, envs: &[(&str, &str)], expected: EffectiveState) {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    if disabled {
        write_disabled_flag(home.path());
    }

    let status = status_effective_state(home.path(), cwd.path(), envs);
    assert_eq!(
        status, expected,
        "aegis status reported an unexpected effective state"
    );

    for hook in SESSION_START_HOOKS {
        let notice = notice_effective_state(home.path(), hook, envs);
        assert_eq!(
            notice, status,
            "{hook} disagrees with the authoritative aegis status effective state"
        );
    }
}

#[test]
fn an_enabled_toggle_outside_ci_reports_enforcing_on_both_surfaces() {
    assert_parity(false, &[], EffectiveState::Enforcing);
}

#[test]
fn a_disabled_toggle_outside_ci_reports_disabled_passthrough_on_both_surfaces() {
    assert_parity(true, &[], EffectiveState::DisabledPassthrough);
}

#[test]
fn a_disabled_toggle_under_ci_reports_the_override_on_both_surfaces() {
    assert_parity(true, &[("CI", "true")], EffectiveState::CiOverride);
}

#[test]
fn falsy_aegis_ci_keeps_the_notice_and_status_on_disabled_passthrough() {
    assert_parity(
        true,
        &[("AEGIS_CI", "false"), ("CI", "true")],
        EffectiveState::DisabledPassthrough,
    );
}

/// `JENKINS_URL` is the one marker both implementations read as "set and
/// non-empty" rather than as a truthy word, so it needs its own case: a
/// truthiness check on either side would silently drop Jenkins.
#[test]
fn a_non_empty_jenkins_url_overrides_a_disabled_toggle_on_both_surfaces() {
    assert_parity(
        true,
        &[("JENKINS_URL", "https://jenkins.example/job/1")],
        EffectiveState::CiOverride,
    );
}

#[test]
fn an_enabled_toggle_under_ci_never_claims_an_override_on_either_surface() {
    assert_parity(false, &[("CI", "true")], EffectiveState::Enforcing);
}
