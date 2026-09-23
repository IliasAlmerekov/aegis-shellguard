//! Wrapper-routing regression tests for issue #430: a subshell, brace group,
//! command substitution/backtick, or reserved-word prefix used to hide an
//! interpreter from routing entirely. Split from `router::tests` to stay
//! under the file-size budget.

use super::*;

// ── #430: routing sees through a wrapper ────────────────────────────────────

#[test]
fn a_subshell_wrapped_script_is_routed() {
    assert_eq!(
        route("(true; python3 script.py)", &[]),
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("script.py"),
        }]
    );
}

#[test]
fn a_brace_group_wrapped_script_is_routed() {
    assert_eq!(
        route("true; { python3 script.py; }", &[]),
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("script.py"),
        }]
    );
}

#[test]
fn a_dollar_paren_command_substitution_script_is_routed() {
    assert_eq!(
        route("echo $(python3 script.py)", &[]),
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("script.py"),
        }]
    );
}

#[test]
fn a_backtick_command_substitution_script_is_routed() {
    assert_eq!(
        route("echo `python3 script.py`", &[]),
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("script.py"),
        }]
    );
}

#[test]
fn an_if_then_wrapped_script_is_routed() {
    assert_eq!(
        route("if true; then python3 script.py; fi", &[]),
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("script.py"),
        }]
    );
}

#[test]
fn a_while_do_wrapped_script_is_routed() {
    assert_eq!(
        route("while true; do python3 script.py; done", &[]),
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("script.py"),
        }]
    );
}

#[test]
fn a_case_arm_wrapped_script_is_routed() {
    assert_eq!(
        route("case x in x) python3 script.py;; esac", &[]),
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("script.py"),
        }]
    );
}

#[test]
fn else_elif_bang_and_time_prefixes_are_all_routed() {
    for cmd in [
        "if false; then true; else python3 script.py; fi",
        "if false; then true; elif true; then python3 script.py; fi",
        "! python3 script.py",
        "time python3 script.py",
    ] {
        assert_eq!(
            route(cmd, &[]),
            vec![RoutedTarget::ScriptFile {
                language: SourceLanguage::Python,
                path: PathBuf::from("script.py"),
            }],
            "must route through wrapper in: {cmd}"
        );
    }
}

// ── A wrapper with no routable command inside stays empty ──────────────────

#[test]
fn a_subshell_with_no_interpreter_routes_nothing() {
    assert_eq!(route("(echo ok)", &[]), Vec::new());
}

#[test]
fn an_if_then_with_no_interpreter_routes_nothing() {
    assert_eq!(route("if true; then echo ok; fi", &[]), Vec::new());
}

#[test]
fn a_command_substitution_with_no_interpreter_routes_nothing() {
    assert_eq!(route("echo $(date)", &[]), Vec::new());
}

// ── A cd inside the wrapped body degrades a relative target found there ────
//
// A bare `(...)`/`{...}` wrapper around the whole stage is caught earlier by
// `parse_cd_like`'s own wrapper branch (S3) and never reaches wrapper-stage
// routing at all. A command substitution does: `parse_cd_like` only strips a
// literal `{}`/`()` wrapper around the *entire* stage, so `echo $(cd sub;
// python3 script.py)` falls through to `route_wrapped_stage`, which must not
// trust the parent cwd for a relative target once the extracted body is
// found to `cd` on its own.
#[test]
fn a_cd_inside_a_command_substitution_degrades_a_relative_script_found_there() {
    let targets = route("echo $(cd sub; python3 script.py)", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}
