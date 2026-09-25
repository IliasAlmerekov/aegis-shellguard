//! `rg --pre` and `ag --pager` name a command the tool runs to preprocess
//! its input, not a value it merely reads — read the same way `man -P`
//! already is (GHSA-xj54 follow-up). Both `rg` and `ag` sit on the
//! unclaimed net's own `NAME_ONLY_PROGRAMS` list, so without this dispatch a
//! command smuggled through either flag was invisible to routing. `use
//! super::*` reaches the same `router` test imports its sibling files use.

use super::*;

// ── A separate-token or `=`-glued value routes as its own launcher operand
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn rg_pre_separate_token_path_routes() {
    assert_eq!(
        route("rg --pre ./pyx foo .", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

#[test]
fn rg_pre_equals_glued_path_routes() {
    assert_eq!(
        route("rg --pre=./pyx foo .", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

#[test]
fn ag_pager_separate_token_path_routes() {
    assert_eq!(
        route("ag --pager ./pyx foo", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

#[test]
fn ag_pager_equals_glued_path_routes() {
    assert_eq!(
        route("ag --pager=./pyx foo", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

// ── An interpreter-named value degrades, same as any other executor value
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn rg_pre_naming_an_interpreter_degrades() {
    assert_eq!(
        route("rg --pre 'python3 ./evil.py' foo .", &[]),
        vec![unresolved_dynamic()]
    );
}

// ── Ordinary rg/ag invocations, including a value on `--pre-glob` (which
// must not be misread as `--pre`), auto-approve ────────────────────────────

#[test]
fn ordinary_rg_and_ag_invocations_auto_approve() {
    for command in [
        "rg foo .",
        "rg -n TODO src/",
        "ag foo",
        "rg --pre-glob '*.gz' foo .",
    ] {
        assert_eq!(route(command, &[]), Vec::new(), "{command}");
    }
}

// ── A bare `--` ends rg's and ag's own option parsing, so a `--pre`/
// `--pager`-shaped word past it is a positional argument, not the option
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn rg_and_ag_option_words_past_bare_double_dash_auto_approve() {
    for command in ["rg -- --pre ./pyx foo .", "ag -- --pager ./pyx foo"] {
        assert_eq!(route(command, &[]), Vec::new(), "{command}");
    }
}

#[test]
fn rg_pre_before_bare_double_dash_still_routes() {
    assert_eq!(
        route("rg --pre ./pyx -- foo .", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}
