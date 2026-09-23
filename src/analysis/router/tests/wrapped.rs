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
// `route_wrapped_stage` walks every wrapper body with its own cwd tracking,
// seeded from whatever cwd the caller already had (issue #384 R1) — a `cd`
// found inside a command substitution's body must not leave a sibling
// relative target in the same body trusting the parent's cwd.
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

// ── #384 R1: a cd sharing a wrapper body with a routable command must not
// suppress routing of that command ─────────────────────────────────────────
//
// A whole-stage `(...)`/`{...}` wrap containing *any* cd-like segment used to
// be swallowed by `parse_cd_like`'s own wrapper-stripping branch before this
// fix: `route_list_segment` folded the cd into cwd state and returned without
// ever routing the rest of the body. A cd running *after* the routable
// command must not affect that command's own resolution either (it reads
// against the cwd this segment entered with, not the cwd the cd sets on the
// way out) — the router has no notion of statement order inside a fold, only
// "did this body perform a cd at all".

#[test]
fn a_subshell_whose_body_also_cds_still_routes_its_earlier_script() {
    let targets = route("(python3 ./evil.py; cd /)", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("./evil.py"),
        }]
    );
}

#[test]
fn a_brace_group_whose_body_also_cds_still_routes_its_earlier_script() {
    let targets = route("{ python3 ./evil.py; cd /; }", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("./evil.py"),
        }]
    );
}

#[test]
fn a_brace_group_whose_body_also_pushds_still_routes_its_earlier_script() {
    let targets = route("{ python3 ./evil.py; pushd /; }", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("./evil.py"),
        }]
    );
}

#[test]
fn a_cd_tainted_subshell_piped_into_cat_still_routes_its_earlier_script() {
    let targets = route("(python3 ./evil.py; cd /) | cat", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("./evil.py"),
        }]
    );
}

// ── #384 R1: a whole-stage-wrapped cd that leads its body degrades the
// outer walk instead of silently dropping the rest of the body ────────────

#[test]
fn a_subshell_leading_with_a_literal_cd_joins_and_degrades_the_outer_walk() {
    let targets = route("(cd -- d1 && python3 ./sub/evil.py)", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("d1").join("./sub/evil.py"),
        }]
    );
}

#[test]
fn a_subshell_leading_with_a_non_literal_cd_degrades_its_own_body() {
    let targets = route("(cd d1 && python3 ./sub/evil.py)", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}

#[test]
fn a_subshell_with_a_cd_and_the_command_separated_by_a_semicolon_degrades() {
    // Only an unbroken `&&` chain trusts a cd's effect on what follows it,
    // even inside a wrapper body (ADR-022 §6).
    let targets = route("(cd d1; python3 ./sub/evil.py)", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}

#[test]
fn a_brace_group_leading_with_a_cd_and_a_semicolon_degrades() {
    let targets = route("{ cd d1; python3 ./sub/evil.py; }", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}

#[test]
fn an_untrusted_outer_cd_stays_degraded_through_a_literal_cd_inside_a_brace_group() {
    // "Degraded never recovers" (ADR-022 §6): the outer `cd d1` (no `--`) is
    // already untrusted, so the brace group's own literal `cd -- sub` must
    // not resurrect trust in the joined path.
    let targets = route("cd d1 && { cd -- sub && python3 ./evil.py; }", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}

// ── An inline body's own quoting survives wrapper extraction ───────────────
//
// A wrapper body is real shell source, routed by splitting it with
// `aegis_parser::list_segments` and feeding each raw stage straight to
// `route_single_stage` — never through a dequoted/rejoined copy — so a
// literal `'` inside an inline `-c` body is not mistaken for a second-pass
// quote delimiter (issue #430 acceptance: `open('x','w')` must reach the
// worker byte-for-byte, not corrode into `open(x,w)`).
#[test]
fn an_inline_body_with_single_quotes_keeps_its_quoting_through_a_subshell() {
    let targets = route(r#"(python3 -c "open('x','w')")"#, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Inline {
            language: SourceLanguage::Python,
            source: "open('x','w')".to_owned(),
        }]
    );
}
