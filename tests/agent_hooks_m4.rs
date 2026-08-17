//! M4 — Hook panic fail-closed integration tests.
//!
//! These drive the real `aegis hook` subcommand and the installed per-agent
//! `Hook` scripts as child processes, asserting only what an agent or a human
//! can observe: the JSON response on stdout, the process exit status, and the
//! stderr line. Split out of `tests/agent_hooks.rs` to keep that file under the
//! 800-line budget (M5.1 quality gate).

mod support;

use std::fs;
use std::io::Write;
use std::process::{Command, Output, Stdio};

use serde_json::Value;
use tempfile::TempDir;

use support::agent_hooks::run_script_with_env;

/// Run the real `aegis hook` subcommand as a child process with a panic
/// injected through `AEGIS_TEST_PANIC_HOOK` (a `cfg(debug_assertions)`-only
/// read) plus any extra environment overrides. Returns the child's output.
fn run_hook_process(envs: &[(&str, &str)]) -> Output {
    let home = TempDir::new().unwrap();
    let input = serde_json::json!({ "tool_input": { "command": "rm -rf /tmp/x" } }).to_string();

    let mut command = Command::new(env!("CARGO_BIN_EXE_aegis"));
    command
        .arg("hook")
        .env("HOME", home.path())
        .env("AEGIS_TEST_PANIC_HOOK", "1")
        // The child must not inherit an ambient opt-in (CI sets RUST_BACKTRACE=1
        // for cargo test). Remove AEGIS_DEBUG so the tests control opt-in solely
        // through the RUST_BACKTRACE override they pass in.
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

/// The `Hook` boundary must convert a contained panic into the ordinary deny
/// shape rather than dying silently (M4). This drives the real `aegis hook`
/// subcommand as a child process with a panic injected through a dedicated
/// environment variable that exists only in non-release builds.
#[test]
fn hook_contained_panic_emits_deny_and_exits_zero() {
    // Pin RUST_BACKTRACE=0 so the child deterministically does not opt into the
    // debug payload/location lines regardless of the ambient environment (CI
    // sets RUST_BACKTRACE=1 for cargo test) — the assertion below requires
    // exactly one stderr line.
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

    // The human must see exactly one deterministic stderr line saying the panic
    // was contained (user story 5) — nothing more, so a regression that printed
    // two lines would fail.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.trim(),
        "aegis: internal hook panic contained",
        "contained panic must print exactly one deterministic stderr line; stderr=\n{stderr}"
    );
}

/// The opt-in debug detail must not leak when `RUST_BACKTRACE=0` — a value that
/// conventionally disables backtraces must not be read as opt-in (M4).
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

/// Opting in through `RUST_BACKTRACE=1` must append the payload and location so
/// a developer can diagnose a contained panic (user story 6).
#[test]
fn hook_contained_panic_includes_payload_when_backtrace_opted_in() {
    let output = run_hook_process(&[("RUST_BACKTRACE", "1")]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("aegis: panic payload: injected hook panic for test"),
        "RUST_BACKTRACE=1 must append the panic payload; stderr=\n{stderr}"
    );
}

/// Create an executable stub `aegis` binary in a fresh temp dir that runs
/// `body` when invoked. Returns the temp dir (kept alive for the stub's
/// lifetime) and the stub's absolute path.
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

/// The script-level fail-closed layer must emit its own deny response when the
/// binary terminates abnormally (non-zero exit) — the failure class an
/// in-process unwind guard structurally cannot cover (M4). One test per agent.
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

/// Empty stdout with exit status 0 is a legitimate noop and must be forwarded
/// as silence — the fail-closed layer must not reinterpret the existing noop
/// contract (M4, user story 14/15). One test per agent for parity.
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

/// A zero exit status with a response body must forward that body unchanged, so
/// exactly one deny reaches the agent and the two layers never double-print
/// (M4, user story 16). The script must not append its own abnormal-termination
/// deny on top of a body the binary already produced. One test per agent.
fn assert_forwards_body(script: &str) {
    let home = TempDir::new().unwrap();
    // The stub exits 0 with a deny body — the case where the in-process layer
    // already spoke and the script must relay it verbatim, never speak again.
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
    // The script relays the binary's stdout verbatim (one trailing newline from
    // its own printf), so the agent sees exactly one deny — never a second,
    // script-authored one. The exact-equality assert is the whole pin: any
    // double-print (the body twice, or a script-authored abnormal-termination
    // deny appended) makes stdout longer than `{body}\n` and fails it.
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
