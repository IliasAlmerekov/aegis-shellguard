//! A quoted multi-word command string handed to an unenumerated wrapper
//! (`script -c "exec python3 ./evil.py"`) can open with a shell prefix word
//! — `exec`, a bare `NAME=value` assignment, `nohup`, ... — ahead of the
//! interpreter it actually runs (issue #384/#430, review comment
//! 4091038690). Split out of `unclaimed_interpreter_net.rs` to keep that
//! file under this project's line budget. `use super::*` reaches the same
//! `router` test imports (`RoutedTarget`, `SourceLanguage`, `route`, ...)
//! its sibling files use.

use super::*;

#[test]
fn script_dash_c_quoted_exec_prefixed_interpreter_command_is_routed() {
    // `exec` inside the quoted command string still runs whatever follows
    // it, so the interpreter behind it must be as visible as it is with no
    // prefix at all.
    assert_eq!(
        route(r#"script -c "exec python3 ./evil.py""#, &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn script_dash_c_quoted_env_assignment_prefixed_interpreter_command_is_routed() {
    // A bare `NAME=value` assignment ahead of the interpreter inside the
    // quoted command string is exactly as opaque as `exec` is — the
    // assignment does not change which program runs.
    assert_eq!(
        route(r#"script -c "FOO=bar python3 ./evil.py""#, &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn script_dash_c_quoted_nohup_prefixed_interpreter_command_is_routed() {
    assert_eq!(
        route(r#"script -c "nohup python3 ./evil.py""#, &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn script_dash_c_quoted_prefix_word_with_no_interpreter_is_not_routed() {
    // "exec" ahead of a benign command carries no interpreter to catch, the
    // same way a plain quoted benign command does not.
    assert_eq!(route(r#"ssh host "exec echo hello""#, &[]), Vec::new());
}
