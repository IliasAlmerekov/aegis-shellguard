mod support;

use std::fs;
use std::io::Write;
use std::process::{Command, Output, Stdio};

use serde_json::Value;
use tempfile::TempDir;

use support::agent_hooks::run_script_with_env;

fn run_hook_process(envs: &[(&str, &str)]) -> Output {
    let home = TempDir::new().unwrap();
    let input = serde_json::json!({ "tool_input": { "command": "rm -rf /tmp/x" } }).to_string();

    let mut command = Command::new(env!("CARGO_BIN_EXE_aegis"));
    command
        .arg("hook")
        .env("HOME", home.path())
        .env("AEGIS_TEST_PANIC_HOOK", "1")
        .env_remove("AEGIS_DEBUG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        command.env(key, value);
    }
    let mut child = command.spawn().expect("aegis hook must spawn");
    child
        .stdin
        .as_mut()
        .expect("hook stdin")
        .write_all(input.as_bytes())
        .expect("hook input");
    child.wait_with_output().expect("aegis hook must finish")
}

#[test]
fn hook_contained_panic_emits_deny_and_exits_zero() {
    let output = run_hook_process(&[("RUST_BACKTRACE", "0")]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "a contained panic must still exit 0 so the agent parses the JSON decision; stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).expect(
        "a contained panic must emit valid deny JSON on stdout; stdout was empty or unparseable",
    );

    assert_eq!(
        json["hookSpecificOutput"]["permissionDecision"], "deny",
        "a contained panic must deny (fail closed); json=\n{json}"
    );

    let fixed_reason = "aegis hook failed internally; refusing to run command unscanned";
    assert_eq!(
        json["reason"], fixed_reason,
        "top-level `reason` must carry the fixed panic reason; json=\n{json}"
    );
    assert_eq!(
        json["hookSpecificOutput"]["permissionDecisionReason"], fixed_reason,
        "permissionDecisionReason must mirror the fixed panic reason; json=\n{json}"
    );
    assert!(
        json.get("decision").is_none(),
        "a contained panic must not emit a top-level legacy `decision` field; json=\n{json}"
    );
    assert!(
        json["hookSpecificOutput"].get("updatedInput").is_none(),
        "a contained panic must not emit updatedInput; json=\n{json}"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.trim(),
        "aegis: internal hook panic contained",
        "contained panic must print exactly one deterministic stderr line; stderr=\n{stderr}"
    );
}

#[test]
fn hook_contained_panic_omits_payload_when_backtrace_is_zero() {
    let output = run_hook_process(&[("RUST_BACKTRACE", "0")]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("aegis: internal hook panic contained"),
        "the contained line must still print; stderr=\n{stderr}"
    );
    assert!(
        !stderr.contains("aegis: panic payload"),
        "RUST_BACKTRACE=0 must not opt into the debug payload line; stderr=\n{stderr}"
    );
}

#[test]
fn hook_contained_panic_includes_payload_when_backtrace_opted_in() {
    let output = run_hook_process(&[("RUST_BACKTRACE", "1")]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("aegis: panic payload: injected hook panic for test"),
        "RUST_BACKTRACE=1 must append the panic payload; stderr=\n{stderr}"
    );
}

fn stub_aegis_bin(body: &str) -> (TempDir, String) {
    let dir = TempDir::new().unwrap();
    let stub = dir.path().join("aegis");
    fs::write(&stub, format!("#!/bin/sh\n{body}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&stub).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&stub, perms).unwrap();
    }
    (dir, stub.display().to_string())
}

fn assert_abnormal_deny(script: &str) {
    let home = TempDir::new().unwrap();
    let (_dir, stub) = stub_aegis_bin("exit 3");
    let stdin_json =
        serde_json::json!({ "tool_input": { "command": "rm -rf /tmp/x" } }).to_string();

    let output = run_script_with_env(
        script,
        home.path(),
        &[],
        Some(stdin_json.as_str()),
        &[("AEGIS_BIN", &stub)],
    );

    assert_eq!(
        output.status.code(),
        Some(0),
        "the script must exit 0 so the agent parses the JSON decision; stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).expect(
        "the script must emit valid deny JSON on abnormal termination; stdout was empty or unparseable",
    );
    assert_eq!(
        json["hookSpecificOutput"]["permissionDecision"], "deny",
        "the script must deny (fail closed) on abnormal termination; json=\n{json}"
    );
    let reason = "aegis hook terminated abnormally; refusing to run command unscanned";
    assert_eq!(
        json["reason"], reason,
        "the script's deny reason must be distinct from the binary-unavailable reason; json=\n{json}"
    );
    assert_eq!(
        json["hookSpecificOutput"]["permissionDecisionReason"], reason,
        "permissionDecisionReason must mirror the script's deny reason; json=\n{json}"
    );
}

#[test]
fn claude_hook_fails_closed_when_binary_terminates_abnormally() {
    assert_abnormal_deny("hooks/claude-code.sh");
}

#[test]
fn codex_hook_fails_closed_when_binary_terminates_abnormally() {
    assert_abnormal_deny("hooks/codex-pre-tool-use.sh");
}

fn assert_noop_silence(script: &str) {
    let home = TempDir::new().unwrap();
    let (_dir, stub) = stub_aegis_bin("exit 0");
    let stdin_json =
        serde_json::json!({ "tool_input": { "command": "rm -rf /tmp/x" } }).to_string();

    let output = run_script_with_env(
        script,
        home.path(),
        &[],
        Some(stdin_json.as_str()),
        &[("AEGIS_BIN", &stub)],
    );

    assert_eq!(
        output.status.code(),
        Some(0),
        "a noop must still exit 0; stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "empty stdout with exit 0 must stay a silent noop; stdout=\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn claude_hook_forwards_silence_when_binary_exits_zero_without_output() {
    assert_noop_silence("hooks/claude-code.sh");
}

#[test]
fn codex_hook_forwards_silence_when_binary_exits_zero_without_output() {
    assert_noop_silence("hooks/codex-pre-tool-use.sh");
}

fn assert_forwards_body(script: &str) {
    let home = TempDir::new().unwrap();
    let body = r#"{"reason":"aegis hook denied; refusing to run command unscanned","hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"aegis hook denied; refusing to run command unscanned"}}"#;
    let (_dir, stub) = stub_aegis_bin(&format!("printf '%s\\n' '{body}'\nexit 0"));
    let stdin_json =
        serde_json::json!({ "tool_input": { "command": "rm -rf /tmp/x" } }).to_string();

    let output = run_script_with_env(
        script,
        home.path(),
        &[],
        Some(stdin_json.as_str()),
        &[("AEGIS_BIN", &stub)],
    );

    assert_eq!(
        output.status.code(),
        Some(0),
        "forwarding a body must still exit 0; stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = format!("{body}\n");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        expected,
        "the script must forward the binary's body unchanged, exactly once; stdout=\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn claude_hook_forwards_binary_deny_body_unchanged() {
    assert_forwards_body("hooks/claude-code.sh");
}

#[test]
fn codex_hook_forwards_binary_deny_body_unchanged() {
    assert_forwards_body("hooks/codex-pre-tool-use.sh");
}
