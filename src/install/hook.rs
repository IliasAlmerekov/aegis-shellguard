use std::io::{Read, Write};

use serde_json::Value;

use super::shell_quote;

/// The fixed, detail-free reason used when a panic is contained at the `Hook`
/// boundary. The payload is never interpolated into the response, so a panic
/// carrying paths, command fragments, or internal state cannot steer the deny
/// shape or leak into a model's context (M4).
const CONTAINED_PANIC_REASON: &str =
    "aegis hook failed internally; refusing to run command unscanned";

/// The deterministic stderr line printed when a panic is contained. Kept as a
/// constant so stderr assertions do not drift with toolchain changes to the
/// default panic message (M4).
const CONTAINED_PANIC_STDERR: &str = "aegis: internal hook panic contained";

/// Run the Claude Code `PreToolUse` hook and rewrite unwrapped Bash commands
/// through `aegis --command`.
///
/// The `Hook` boundary is the single place a catch is installed: an unwind
/// anywhere across the stdin-read + outcome production is converted into the
/// ordinary deny shape rather than dying silently (M4). Rendering and writing
/// the response happen outside the guard, so a panic in the write path cannot
/// defeat the guard. The exit code stays 0 for allow, noop, ordinary deny, and
/// contained panic alike — with these agent clients only exit 0 gets the JSON
/// decision parsed, so a non-zero exit would demote a deny into a non-blocking
/// hook error and let the command run.
pub(crate) fn run_hook() -> i32 {
    install_hook_panic_hook();

    let outcome = std::panic::catch_unwind(|| {
        // Test-only panic injection, compiled only in non-release builds so a
        // shipped binary contains no way to induce a panic path through the
        // environment (M4). The closure captures no mutable borrows, so no
        // `AssertUnwindSafe` is required.
        #[cfg(debug_assertions)]
        if std::env::var_os("AEGIS_TEST_PANIC_HOOK").is_some() {
            panic!("injected hook panic for test");
        }
        hook_response_from_stdin()
    });

    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(_) => HookOutcome::Deny(hook_deny_output(CONTAINED_PANIC_REASON.to_string())),
    };

    match outcome {
        HookOutcome::Allow(output) | HookOutcome::Deny(output) => {
            write_hook_output(&output);
        }
        HookOutcome::Noop => {}
    }

    0
}

/// Install a minimal panic hook scoped to `Hook` mode. It prints one
/// deterministic stderr line and appends the payload/location only when the
/// user has opted into backtrace/debug output through the environment. The
/// hook is not restored: the process exits immediately after a contained panic.
/// Scope is `Hook` mode only — no panic hook is installed in `main`, so the
/// shell-proxy, `watch`, and TUI paths keep the full default panic output.
fn install_hook_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        write_stderr(CONTAINED_PANIC_STDERR);
        if std::env::var_os("RUST_BACKTRACE").is_some() || std::env::var_os("AEGIS_DEBUG").is_some()
        {
            write_stderr(&format!(
                "aegis: panic payload: {}",
                panic_payload_text(info.payload())
            ));
            if let Some(location) = info.location() {
                write_stderr(&format!("aegis: panic location: {location}"));
            }
        }
    }));
}

/// Render a panic payload as text for the opt-in debug stderr line. A non-string
/// payload yields a stable placeholder — the same reason is used regardless of
/// what the panic happens to carry (M4).
fn panic_payload_text(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

/// Write a line to stderr via an explicit locked write plus flush. A write
/// error is ignored silently — a closed stderr must not itself become a double
/// panic that aborts the process.
fn write_stderr(line: &str) {
    let stderr = std::io::stderr();
    let mut lock = stderr.lock();
    let _ = writeln!(lock, "{line}");
    let _ = lock.flush();
}

/// Write the hook response to stdout via an explicit locked write plus flush.
/// A write error (e.g. a closed pipe) is ignored silently and does not alter the
/// exit code — a closed pipe must not itself become the crash that defeats the
/// guard (M4).
fn write_hook_output(output: &Value) {
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    let _ = writeln!(lock, "{output}");
    let _ = lock.flush();
}

#[derive(Debug)]
pub(crate) enum HookOutcome {
    Allow(Value),
    Deny(Value),
    Noop,
}

fn hook_response_from_stdin() -> HookOutcome {
    let mut input = String::new();
    if let Err(err) = std::io::stdin().read_to_string(&mut input) {
        return HookOutcome::Deny(hook_deny_output(format!(
            "aegis could not read hook input: {err}"
        )));
    }

    hook_response_value(&input)
}

fn hook_response_value(input: &str) -> HookOutcome {
    let input: Value = match serde_json::from_str(input) {
        Ok(value) => value,
        Err(err) => {
            return HookOutcome::Deny(hook_deny_output(format!("invalid hook input: {err}")));
        }
    };

    let Some(root) = input.as_object() else {
        return HookOutcome::Deny(hook_deny_output(
            "invalid hook input: expected a JSON object".to_string(),
        ));
    };

    let Some(tool_input) = root.get("tool_input") else {
        return HookOutcome::Deny(hook_deny_output(
            "invalid hook input: missing tool_input".to_string(),
        ));
    };

    let Some(tool_input) = tool_input.as_object() else {
        return HookOutcome::Deny(hook_deny_output(
            "invalid hook input: tool_input must be a JSON object".to_string(),
        ));
    };

    let Some(command_value) = tool_input.get("command") else {
        return HookOutcome::Noop;
    };

    let Some(command) = command_value.as_str() else {
        return HookOutcome::Deny(hook_deny_output(
            "invalid hook input: tool_input.command must be a string".to_string(),
        ));
    };

    // A command already in canonical wrapper form must pass through untouched —
    // re-wrapping would double-intercept. A command that merely begins with the
    // `aegis` word but is NOT a canonical wrapper is rejected: it could be a
    // half-formed or evasive wrapper, and wrapping it again would hide the
    // malformation. Fail closed with a clear reason instead of guessing.
    if is_canonical_aegis_wrapper(command) {
        return HookOutcome::Noop;
    }
    if starts_with_aegis_word(command) {
        return HookOutcome::Deny(hook_deny_output(
            "invalid aegis wrapper syntax; issue the command unwrapped and aegis will rewrite it"
                .to_string(),
        ));
    }

    let mut updated_input = tool_input.clone();
    updated_input.insert(
        "command".to_string(),
        Value::String(format!("aegis --command {}", shell_quote(command))),
    );

    HookOutcome::Allow(serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow",
            "permissionDecisionReason": "aegis intercept",
            "updatedInput": updated_input,
        }
    }))
}

fn hook_deny_output(reason: String) -> Value {
    serde_json::json!({
        // Claude reads the top-level `reason` for the deny message while Codex
        // reads `hookSpecificOutput.permissionDecisionReason`. Emit both so the
        // deny reason is visible in either agent. The structured
        // `permissionDecision` form is intentional — a top-level legacy
        // `decision` field is deliberately NOT emitted.
        "reason": reason.clone(),
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": reason,
        }
    })
}

/// The canonical command prefix Aegis rewrites Bash commands to.
const AEGIS_WRAPPER_PREFIX: &str = "aegis --command ";

/// True when `command` begins with the bare `aegis` executable word — either
/// exactly `aegis` or `aegis` followed by whitespace. Used to distinguish an
/// already-aegis invocation from an unrelated command like `aegisctl`.
fn starts_with_aegis_word(command: &str) -> bool {
    command
        .strip_prefix("aegis")
        .is_some_and(|rest| rest.is_empty() || rest.chars().next().is_some_and(char::is_whitespace))
}

/// True only when `command` is exactly `aegis --command <arg>` where `<arg>` is
/// the POSIX single-quoted form `shell_quote` itself produces — i.e. re-quoting
/// the decoded argument reproduces the command byte-for-byte. This rejects
/// half-formed wrappers (`aegis --command '`) that merely share the prefix.
fn is_canonical_aegis_wrapper(command: &str) -> bool {
    let Some(payload) = command.strip_prefix(AEGIS_WRAPPER_PREFIX) else {
        return false;
    };
    match decode_single_quoted(payload) {
        Some(decoded) => shell_quote(&decoded) == payload,
        None => false,
    }
}

/// Decode a single POSIX single-quoted token of the exact shape `shell_quote`
/// emits: `'...'` with embedded single quotes encoded as the close-reopen
/// sequence `'\''`. Returns `None` for anything that is not one well-formed
/// single-quoted token (stray quotes, missing terminator, trailing content).
fn decode_single_quoted(payload: &str) -> Option<String> {
    let inner = payload.strip_prefix('\'')?;
    let chars: Vec<char> = inner.chars().collect();
    let mut decoded = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\'' {
            // Final closing quote: must be the last character.
            if i == chars.len() - 1 {
                return Some(decoded);
            }
            // Otherwise the only legal continuation is the `'\''` escape.
            if chars.get(i + 1) == Some(&'\\')
                && chars.get(i + 2) == Some(&'\'')
                && chars.get(i + 3) == Some(&'\'')
            {
                decoded.push('\'');
                i += 4;
                continue;
            }
            return None;
        }
        decoded.push(chars[i]);
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_rewrites_plain_command_with_shell_quote() {
        let output =
            match hook_response_value(r#"{"tool_input":{"command":"git commit -m 'fix: hello'"}}"#)
            {
                HookOutcome::Allow(output) => output,
                other => panic!("expected rewrite output, got {other:?}"),
            };
        let rewritten = format!(
            "aegis --command {}",
            shell_quote("git commit -m 'fix: hello'")
        );

        let expected = serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "permissionDecisionReason": "aegis intercept",
                "updatedInput": {
                    "command": rewritten
                }
            }
        });

        assert_eq!(output, expected);
    }

    #[test]
    fn hook_skips_already_wrapped_command() {
        assert!(matches!(
            hook_response_value(r#"{"tool_input":{"command":"aegis --command 'rm -rf /tmp'"}}"#),
            HookOutcome::Noop
        ));
    }

    #[test]
    fn hook_skips_missing_command_field() {
        assert!(matches!(
            hook_response_value(r#"{"tool_input":{}}"#),
            HookOutcome::Noop
        ));
    }

    #[test]
    fn hook_rejects_malformed_json_input() {
        assert!(matches!(
            hook_response_value(r#"{"tool_input":{"command":"#),
            HookOutcome::Deny(_)
        ));
    }

    #[test]
    fn hook_rejects_non_object_tool_input() {
        assert!(matches!(
            hook_response_value(r#"{"tool_input":"rm -rf /"}"#),
            HookOutcome::Deny(_)
        ));
    }

    #[test]
    fn hook_does_not_skip_aegisctl_commands() {
        assert!(matches!(
            hook_response_value(r#"{"tool_input":{"command":"aegisctl status"}}"#),
            HookOutcome::Allow(_)
        ));
    }

    #[test]
    fn hook_denies_non_canonical_aegis_wrapper() {
        // Begins with the `aegis` word but is not a canonical wrapper — must
        // fail closed rather than be re-wrapped or passed through.
        match hook_response_value(r#"{"tool_input":{"command":"aegis --command '"}}"#) {
            HookOutcome::Deny(output) => {
                assert_eq!(output["hookSpecificOutput"]["permissionDecision"], "deny");
                assert!(
                    output["hookSpecificOutput"]["permissionDecisionReason"]
                        .as_str()
                        .unwrap()
                        .contains("invalid aegis wrapper syntax")
                );
            }
            other => panic!("expected deny, got {other:?}"),
        }
    }

    #[test]
    fn hook_denies_bare_aegis_subcommand_that_is_not_canonical() {
        assert!(matches!(
            hook_response_value(r#"{"tool_input":{"command":"aegis audit"}}"#),
            HookOutcome::Deny(_)
        ));
    }

    #[test]
    fn hook_noops_canonical_wrapper_with_embedded_single_quotes() {
        // Round-trip: wrap a command containing single quotes, then confirm the
        // wrapper is recognized as canonical and passed through untouched.
        let wrapped = format!("aegis --command {}", shell_quote("echo 'oops'"));
        let input = serde_json::json!({ "tool_input": { "command": wrapped } }).to_string();
        assert!(matches!(hook_response_value(&input), HookOutcome::Noop));
    }

    #[test]
    fn is_canonical_aegis_wrapper_round_trips_arbitrary_commands() {
        for cmd in [
            "git status",
            "echo 'hi there'",
            "printf '%s\\n' 'a'\\''b'",
            "rm -rf /tmp/x",
        ] {
            let wrapped = format!("aegis --command {}", shell_quote(cmd));
            assert!(
                is_canonical_aegis_wrapper(&wrapped),
                "{wrapped:?} should be canonical"
            );
        }

        assert!(!is_canonical_aegis_wrapper("aegis --command "));
        assert!(!is_canonical_aegis_wrapper("aegis --command 'unterminated"));
        assert!(!is_canonical_aegis_wrapper("aegis --command 'a' extra"));
    }

    #[test]
    fn panic_payload_text_renders_str_and_string_payloads() {
        assert_eq!(panic_payload_text(&"boom"), "boom");
        assert_eq!(panic_payload_text(&"boom".to_string()), "boom");
    }

    #[test]
    fn panic_payload_text_uses_stable_placeholder_for_non_string_payload() {
        // A non-string panic payload must not steer the response shape: it
        // renders to a stable placeholder, and the deny reason itself is the
        // fixed `CONTAINED_PANIC_REASON` regardless of what the panic carries
        // (M4, user story 8).
        assert_eq!(
            panic_payload_text(&42i32),
            "non-string panic payload",
            "a non-string payload must render to the stable placeholder"
        );
        assert_eq!(
            panic_payload_text(&vec![1u8, 2, 3]),
            "non-string panic payload",
            "an arbitrary non-string payload must render to the stable placeholder"
        );
    }

    #[test]
    fn contained_panic_reason_is_fixed_and_detail_free() {
        // The deny reason for a contained panic is one fixed string with no
        // internal detail, so panic payloads carrying paths, command fragments,
        // or internal state are never fed into a model's context (M4).
        assert_eq!(
            CONTAINED_PANIC_REASON,
            "aegis hook failed internally; refusing to run command unscanned"
        );
        let output = hook_deny_output(CONTAINED_PANIC_REASON.to_string());
        assert_eq!(output["reason"], CONTAINED_PANIC_REASON);
        assert_eq!(
            output["hookSpecificOutput"]["permissionDecisionReason"],
            CONTAINED_PANIC_REASON
        );
    }

    #[test]
    fn deny_output_includes_top_level_reason_for_claude() {
        // Claude reads the top-level `reason` for the deny message; Codex reads
        // `hookSpecificOutput.permissionDecisionReason`. Both must carry the
        // reason so the deny is explained in either agent.
        let output = hook_deny_output("nope".to_string());

        assert_eq!(
            output["reason"], "nope",
            "top-level reason must mirror the deny reason"
        );
        assert_eq!(output["hookSpecificOutput"]["permissionDecision"], "deny");
        assert_eq!(
            output["hookSpecificOutput"]["permissionDecisionReason"],
            "nope"
        );
        // The structured permissionDecision form is intentional; do not also emit
        // a top-level legacy `decision` field.
        assert!(
            output.get("decision").is_none(),
            "top-level `decision` must not be emitted"
        );
    }
}
