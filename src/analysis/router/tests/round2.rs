//! Router unit tests for the #384/#430 round-2 findings (H1 regression, P1
//! recursion bound, G1 grammar gaps, L1 launcher/alias gaps). Split from
//! `router::tests` to stay under the repo's 800-line file-size budget;
//! `use super::*` reaches the same `router` test imports (`RoutedTarget`,
//! `SourceLanguage`, `route`, ...) `router::tests`'s other siblings use.

use super::*;

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
