use std::io::{Read, Write};

use serde_json::Value;

use super::shell_quote;

mod read_only;

const CONTAINED_PANIC_REASON: &str =
    "aegis hook failed internally; refusing to run command unscanned";

const CONTAINED_PANIC_STDERR: &str = "aegis: internal hook panic contained";

pub(crate) fn run_hook() -> i32 {
    install_hook_panic_hook();

    let outcome = std::panic::catch_unwind(|| {
        #[cfg(debug_assertions)]
        #[expect(
            clippy::panic,
            reason = "debug-only test hook that exercises the contained-panic path"
        )]
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

fn install_hook_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        write_stderr(CONTAINED_PANIC_STDERR);
        if env_opt_in("RUST_BACKTRACE") || env_opt_in("AEGIS_DEBUG") {
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

fn env_opt_in(name: &str) -> bool {
    std::env::var(name)
        .map(|value| !value.is_empty() && value != "0")
        .unwrap_or(false)
}

fn panic_payload_text(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

fn write_stderr(line: &str) {
    let stderr = std::io::stderr();
    let mut lock = stderr.lock();
    let _ = writeln!(lock, "{line}");
    let _ = lock.flush();
}

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

    if is_canonical_aegis_wrapper(command) {
        return HookOutcome::Noop;
    }
    if starts_with_aegis_word(command) && !read_only::is_read_only_aegis_command(command) {
        let reason = if is_wrapper_attempt(command) {
            "invalid aegis wrapper syntax; issue the command unwrapped and aegis will rewrite it"
        } else {
            "aegis commands that change Aegis itself are reserved for the human operator; \
             only read-only aegis commands (help, --version, status, audit, snapshot list, \
             config show, config validate) run through the hook. They may be chained with \
             each other or piped into head, tail, grep, wc, jq, or cut, and may redirect \
             output only to /dev/null or another descriptor"
        };
        return HookOutcome::Deny(hook_deny_output(reason.to_string()));
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
        "reason": reason.clone(),
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": reason,
        }
    })
}

const AEGIS_WRAPPER_PREFIX: &str = "aegis --command ";

fn starts_with_aegis_word(command: &str) -> bool {
    command
        .strip_prefix("aegis")
        .is_some_and(|rest| rest.is_empty() || rest.chars().next().is_some_and(char::is_whitespace))
}

fn is_wrapper_attempt(command: &str) -> bool {
    command.split_whitespace().skip(1).any(|arg| {
        arg.starts_with("--command")
            || (arg.starts_with('-') && !arg.starts_with("--") && arg.contains('c'))
    })
}

fn is_canonical_aegis_wrapper(command: &str) -> bool {
    let Some(payload) = command.strip_prefix(AEGIS_WRAPPER_PREFIX) else {
        return false;
    };
    match decode_single_quoted(payload) {
        Some(decoded) => shell_quote(&decoded) == payload,
        None => false,
    }
}

fn decode_single_quoted(payload: &str) -> Option<String> {
    let inner = payload.strip_prefix('\'')?;
    let chars: Vec<char> = inner.chars().collect();
    let mut decoded = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\'' {
            if i == chars.len() - 1 {
                return Some(decoded);
            }
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
    fn hook_names_wrapper_syntax_when_the_wrapper_flag_is_not_first() {
        for command in [
            "aegis -c ls",
            "aegis -cls",
            "aegis -v -c ls",
            "aegis --output json --command ls",
        ] {
            match hook_outcome_for(command) {
                HookOutcome::Deny(output) => assert!(
                    output["reason"]
                        .as_str()
                        .unwrap()
                        .contains("invalid aegis wrapper syntax"),
                    "{command:?} must be denied as a malformed wrapper: {output}"
                ),
                other => panic!("expected {command:?} to be denied, got {other:?}"),
            }
        }
    }

    fn hook_outcome_for(command: &str) -> HookOutcome {
        let input = serde_json::json!({ "tool_input": { "command": command } }).to_string();
        hook_response_value(&input)
    }

    #[test]
    fn hook_wraps_read_only_aegis_invocations() {
        for command in [
            "aegis --help",
            "aegis -h",
            "aegis --version",
            "aegis -V",
            "aegis help",
            "aegis help snapshot",
            "aegis off --help",
            "aegis snapshot prune -h",
            "aegis status",
            "aegis audit",
            "aegis audit --last 20 --format json",
            "aegis snapshot list",
            "aegis config show",
            "aegis config validate --output json",
        ] {
            match hook_outcome_for(command) {
                HookOutcome::Allow(output) => assert_eq!(
                    output["hookSpecificOutput"]["updatedInput"]["command"],
                    format!("aegis --command {}", shell_quote(command)),
                    "{command:?} must be rewritten through the wrapper"
                ),
                other => panic!("expected {command:?} to be wrapped, got {other:?}"),
            }
        }
    }

    #[test]
    fn hook_wraps_read_only_aegis_invocations_with_redirects_pipes_and_chains() {
        for command in [
            "aegis --version 2>&1",
            "aegis status 2>/dev/null",
            "aegis status 2> /dev/null",
            "aegis status >/dev/null 2>&1",
            "aegis status &>/dev/null",
            "aegis --help 2>&1 | head -40",
            "aegis audit --last 50 | grep FS-001",
            "aegis audit --format json|jq .risk",
            "aegis config show | tail -n 5 | wc -l",
            "aegis --version && aegis status",
            "aegis --version; aegis status",
            "aegis --version || aegis status",
            "aegis --version\naegis status",
            "aegis status;",
        ] {
            match hook_outcome_for(command) {
                HookOutcome::Allow(output) => assert_eq!(
                    output["hookSpecificOutput"]["updatedInput"]["command"],
                    format!("aegis --command {}", shell_quote(command)),
                    "{command:?} must be rewritten through the wrapper"
                ),
                other => panic!("expected {command:?} to be wrapped, got {other:?}"),
            }
        }
    }

    #[test]
    fn hook_denies_compound_aegis_commands_outside_the_read_only_shapes() {
        for command in [
            "aegis status | aegis off",
            "aegis status && ls",
            "aegis status | sort -o /tmp/out",
            "aegis status | uniq - /tmp/out",
            "aegis status > /tmp/out",
            "aegis status >> /home/user/.aegis/config.toml",
            "aegis status 2>/tmp/err",
            "aegis status < /dev/null",
            "aegis status & aegis off",
            "aegis status |& head",
            "aegis status | | head",
            "aegis status && && aegis status",
            "aegis status |",
            "aegis status 2>&",
            "aegis status >&file",
            "aegis status | grep 'x'",
        ] {
            assert!(
                matches!(hook_outcome_for(command), HookOutcome::Deny(_)),
                "{command:?} must be denied"
            );
        }
    }

    #[test]
    fn hook_names_the_allowed_compound_shapes_when_a_read_only_command_is_combined() {
        let HookOutcome::Deny(output) = hook_outcome_for("aegis status | sort") else {
            panic!("expected a deny");
        };
        let reason = output["reason"].as_str().unwrap();
        assert!(
            reason.contains("piped into head, tail, grep, wc, jq, or cut"),
            "the reason must name the allowed shapes: {reason}"
        );
    }

    #[test]
    fn hook_denies_bare_aegis_subcommand_that_is_not_canonical() {
        for command in [
            "aegis",
            "aegis off",
            "aegis on",
            "aegis rollback snap-1",
            "aegis snapshot prune --yes",
            "aegis config init",
            "aegis install-hooks --all",
            "aegis setup-shell --remove",
            "aegis hook",
            "aegis watch",
            "aegis -- off --help",
            "aegis status && aegis off",
            "aegis status; aegis off",
            "aegis audit | sh",
            "aegis audit $(aegis off)",
            "aegis audit\naegis off",
        ] {
            match hook_outcome_for(command) {
                HookOutcome::Deny(output) => assert!(
                    output["hookSpecificOutput"]["permissionDecisionReason"]
                        .as_str()
                        .unwrap()
                        .contains("reserved for the human operator"),
                    "{command:?} must be denied as a self-management command: {output}"
                ),
                other => panic!("expected {command:?} to be denied, got {other:?}"),
            }
        }
    }

    #[test]
    fn hook_noops_canonical_wrapper_with_embedded_single_quotes() {
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
        assert!(
            output.get("decision").is_none(),
            "top-level `decision` must not be emitted"
        );
    }
}
