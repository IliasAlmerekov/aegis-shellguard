//! Wrapper-routing tests (#430, #384).

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
    let targets = route("cd d1 && { cd -- sub && python3 ./evil.py; }", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}

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

#[test]
fn a_bash_function_with_empty_parens_routes_its_brace_body() {
    let targets = route("function foo() { python3 ./evil.py; }", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("./evil.py"),
        }]
    );
}

// ── #690: dynamic-argument net strips leading shell keywords and `{` ────────

#[test]
fn read_only_commands_after_shell_keywords_or_brace_do_not_degrade() {
    for cmd in [
        r#"for f in a b; do echo "$f"; done"#,
        r#"for f in a; do cat "$f"; done"#,
        r#"if true; then echo "$f"; fi"#,
        r#"while false; do echo "$f"; done"#,
        r#"{ echo "$f"; }"#,
        r#"while read -r l; do echo "$l"; done < f"#,
        r#"! echo "$f""#,
        r#"! { echo "$f"; }"#,
        r#"then ! echo "$f""#,
        "do",
        "then",
    ] {
        assert_eq!(
            route(cmd, &[]),
            Vec::new(),
            "read-only command or bare keyword must not degrade: {cmd}"
        );
    }
}

#[test]
fn dynamic_program_word_after_shell_keyword_or_brace_still_degrades() {
    for cmd in ["do $X -rf /", "then $CMD", "{ $X; }", "! $X"] {
        assert_eq!(
            route(cmd, &[]),
            vec![unresolved_dynamic()],
            "dynamic program word after keyword must degrade: {cmd}"
        );
    }
}

#[test]
fn non_read_only_command_with_dynamic_arg_after_shell_keyword_still_degrades() {
    for cmd in [
        r#"for f in a b; do rm -rf "$f"; done"#,
        r#"do git commit -m "$MSG""#,
    ] {
        assert_eq!(
            route(cmd, &[]),
            vec![unresolved_dynamic()],
            "non-read-only command with dynamic argv must degrade: {cmd}"
        );
    }
}

#[test]
fn brace_expansion_program_word_still_degrades() {
    assert_eq!(route("{a,b} x", &[]), vec![unresolved_dynamic()]);
}
