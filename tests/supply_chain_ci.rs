use std::fs;
use std::path::{Path, PathBuf};

fn repo_path(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn security_gate_action() -> String {
    fs::read_to_string(repo_path(".github/actions/security-gate/action.yml"))
        .expect("security-gate composite action should be readable")
}

#[test]
fn security_gate_runs_full_cargo_deny_check() {
    let action = security_gate_action();

    assert!(
        action.contains("cargo deny check"),
        "the security gate must run cargo deny check"
    );
    assert!(
        !action.contains("cargo deny check bans licenses sources"),
        "the security gate must not omit advisories from cargo deny check"
    );
}

#[test]
fn security_gate_runs_cargo_audit() {
    assert!(
        security_gate_action().contains("cargo audit"),
        "the security gate must run cargo audit"
    );
}

// The audit and deny steps live in a composite action so that CI and the
// release workflow run the identical checks. These two assertions are what
// keeps that true: without them, either caller could quietly drop the action
// and the tests above would still pass against an action nobody invokes.
#[test]
fn ci_security_job_invokes_the_security_gate() {
    let workflow = fs::read_to_string(repo_path(".github/workflows/ci.yml"))
        .expect("CI workflow should be readable");

    assert!(
        workflow.contains("./.github/actions/security-gate"),
        "CI must run the supply-chain checks through the security-gate action"
    );
}

#[test]
fn release_workflow_invokes_the_security_gate_before_building() {
    let workflow = fs::read_to_string(repo_path(".github/workflows/release.yml"))
        .expect("release workflow should be readable");

    assert!(
        workflow.contains("./.github/actions/security-gate"),
        "the release workflow must run the supply-chain checks on the tagged commit"
    );
    assert!(
        workflow.contains("needs: [admission-quality, admission-security]"),
        "the release build matrix must wait for the Tag admission check"
    );
}
