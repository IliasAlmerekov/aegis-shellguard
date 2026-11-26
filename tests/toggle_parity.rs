mod support;

use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

use support::aegis_bin;
use support::agent_hooks::{CI_MARKER_VARS, run_script_with_env};

const SESSION_START_HOOKS: [&str; 2] = [
    "hooks/claude-session-start.sh",
    "hooks/codex-session-start.sh",
];

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum EffectiveState {
    Enforcing,
    DisabledPassthrough,
    CiOverride,
}

fn write_disabled_flag(home: &Path) {
    fs::create_dir_all(home.join(".aegis")).unwrap();
    fs::write(home.join(".aegis").join("disabled"), "timestamp=x\npid=1\n").unwrap();
}

fn status_effective_state(home: &Path, cwd: &Path, envs: &[(&str, &str)]) -> EffectiveState {
    let mut command = Command::new(aegis_bin());
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
