//! Router unit tests for the #384/#430 round-2 findings (H1 regression, P1
//! recursion bound, G1 grammar gaps, L1 launcher/alias gaps). Split from
//! `router::tests` to stay under the repo's 800-line file-size budget;
//! `use super::*` reaches the same `router` test imports (`RoutedTarget`,
//! `SourceLanguage`, `route`, ...) `router::tests`'s other siblings use.

use super::*;
use std::time::Instant;

fn evil_py_script_file() -> RoutedTarget {
    RoutedTarget::ScriptFile {
        language: SourceLanguage::Python,
        path: PathBuf::from("./evil.py"),
    }
}

// ── H1: a command chained on a heredoc marker's own line used to vanish ────

#[test]
fn heredoc_and_then_chained_exec_is_routed() {
    assert_eq!(
        route("cat <<A && python3 ./evil.py\nhi\nA", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn heredoc_semicolon_chained_exec_is_routed() {
    assert_eq!(
        route("cat <<A; python3 ./evil.py\nhi\nA", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn heredoc_dash_variant_chained_exec_is_routed() {
    assert_eq!(
        route("cat <<-A && python3 ./evil.py\nhi\nA", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn quoted_heredoc_marker_chained_exec_is_routed() {
    assert_eq!(
        route("cat <<'A' && python3 ./evil.py\nhi\nA", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn stacked_heredoc_markers_chained_exec_is_routed() {
    assert_eq!(
        route("cat <<A <<B; python3 ./evil.py\nhi\nA", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn heredoc_chained_exec_with_no_space_before_semicolon_is_routed() {
    assert_eq!(
        route("echo x <<A;python3 ./evil.py\nA", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn heredoc_write_then_exec_reuse_still_wins_over_the_generic_tail_route() {
    // The narrow reuse shape (router.rs's `heredoc_write_then_exec_reuse`)
    // reads the heredoc body directly instead of re-reading the file it was
    // just written to, and must not also produce a second, redundant
    // `ScriptFile` route for the same command via the generic H1 tail path.
    let targets = route("cat > f <<EOF && python3 f\nprint(1)\nEOF", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Inline {
            language: SourceLanguage::Python,
            source: "print(1)".to_owned(),
        }]
    );
}

// ── P1: wrapper-peeling recursion is bounded ───────────────────────────────

#[test]
fn deeply_nested_subshells_finish_quickly_and_degrade_past_the_bound() {
    let nested = format!("{}true{}", "(".repeat(3000), ")".repeat(3000));

    let start = Instant::now();
    let targets = route(&nested, &[]);
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_secs() < 5,
        "3000-level nesting must stay well within a generous bound, took {elapsed:?}"
    );
    assert!(
        targets.iter().any(|t| matches!(
            t,
            RoutedTarget::Unresolved {
                reason: aegis_types::DegradationReason::LimitExceeded
            }
        )),
        "past the wrap-depth bound routing must degrade rather than silently \
         treat the unexamined body as safe: {targets:?}"
    );
}

// ── G1: shell grammar that still hid an interpreter ────────────────────────

#[test]
fn a_leading_redirection_does_not_hide_the_program() {
    assert_eq!(
        route(">out python3 ./evil.py", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_leading_glued_fd_duplication_redirection_does_not_hide_the_program() {
    assert_eq!(
        route("2>&1 python3 ./evil.py", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_leading_redirection_before_a_direct_exec_path_does_not_hide_it() {
    assert_eq!(
        route(">o ./pyx", &[]),
        vec![RoutedTarget::DirectExec {
            path: PathBuf::from("./pyx"),
        }]
    );
}

#[test]
fn stdin_redirected_from_a_literal_file_routes_that_file() {
    assert_eq!(
        route("python3 < ./evil.py", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_posix_function_definition_routes_its_body() {
    assert_eq!(
        route("f(){ python3 ./evil.py; }; f", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_function_keyword_definition_routes_its_body() {
    assert_eq!(
        route("function f { python3 ./evil.py; }; f", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_named_coproc_body_is_routed() {
    assert_eq!(
        route("coproc X { python3 ./evil.py; }", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn input_process_substitution_is_routed() {
    assert_eq!(
        route("cat <(python3 ./evil.py)", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn output_process_substitution_is_routed() {
    assert_eq!(
        route("echo >(python3 ./evil.py)", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_case_fallthrough_double_semicolon_arm_is_routed() {
    assert_eq!(
        route("case a in a) true;;& *) python3 ./evil.py;; esac", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_case_fallthrough_single_ampersand_arm_is_routed() {
    assert_eq!(
        route("case a in a) true;& *) python3 ./evil.py;; esac", &[]),
        vec![evil_py_script_file()]
    );
}

// ── L1: launchers and aliases ───────────────────────────────────────────────

#[test]
fn xargs_launcher_does_not_hide_the_program() {
    assert_eq!(
        route("xargs python3 ./evil.py", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn env_split_string_does_not_hide_the_program() {
    assert_eq!(
        route(r#"env -S "python3 ./evil.py""#, &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn nodejs_debian_alias_routes_like_node() {
    assert_eq!(
        route("nodejs ./evil.js", &[]),
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::JavaScript,
            path: PathBuf::from("./evil.js"),
        }]
    );
}

// ── Regression: safe commands stay safe ─────────────────────────────────────

#[test]
fn known_safe_commands_stay_unrouted_or_unchanged() {
    assert_eq!(route("echo ok", &[]), Vec::new());
    assert_eq!(route("(echo ok)", &[]), Vec::new());
    assert_eq!(route("if true; then echo ok; fi", &[]), Vec::new());
    assert_eq!(route("echo $(date)", &[]), Vec::new());
    assert_eq!(route("ls -la | grep foo; echo done", &[]), Vec::new());
    assert_eq!(route("cat <<EOF\nhello\nEOF", &[]), Vec::new());
    assert_eq!(route("git log --format='%h (%s)'", &[]), Vec::new());
}
