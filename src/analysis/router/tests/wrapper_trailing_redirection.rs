//! Router unit tests pinning that a trailing redirection on a wrapper stage
//! (a brace group, a coprocess, or a function definition) does not hide the
//! wrapped command, and that a function body may be any compound command
//! (`{}`, `()`, `case ... esac`), not only a brace group. Split from
//! `router::tests` to stay under the file-size budget, same as this
//! directory's other siblings.

use super::*;

#[test]
fn route_does_not_panic_on_the_fuzzed_unicode_whitespace_command() {
    // Fuzz crash from CI run 35987983871, kept byte for byte. U+0085 (NEL) is
    // whitespace that takes two bytes in UTF-8, so slicing one byte past it
    // lands inside the character. The same input is the
    // `fuzz/corpus/router/unicode-next-line.txt` seed.
    let command = concat!(
        "ca \u{85}\u{18}stiprc\u{fffd}",
        "||||||||||||||||||./scrip\n",
        r"&\\\\\\\\\& pE",
        "\nEO\n ",
    );
    let _ = route(command, &[("py", "python3")]);
}

fn evil_py_script_file() -> RoutedTarget {
    RoutedTarget::ScriptFile {
        language: SourceLanguage::Python,
        path: PathBuf::from("./evil.py"),
    }
}

// ── a trailing redirect on a brace group or coprocess still routes the body ─

#[test]
fn a_brace_group_with_a_trailing_redirect_routes_its_body() {
    assert_eq!(
        route("{ python3 ./evil.py; } 2>/dev/null", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_named_coproc_with_a_trailing_redirect_routes_its_body() {
    assert_eq!(
        route("coproc X { python3 ./evil.py; } 2>/dev/null", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn an_unnamed_coproc_with_a_trailing_redirect_routes_its_body() {
    assert_eq!(
        route("coproc { python3 ./evil.py; } >log", &[]),
        vec![evil_py_script_file()]
    );
}

// ── a function body may be any compound command, with or without a redirect ─

#[test]
fn a_posix_function_with_a_space_before_its_parens_routes_its_body() {
    assert_eq!(
        route("f () { python3 ./evil.py; }; f", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_posix_function_with_a_subshell_body_routes_its_body() {
    assert_eq!(
        route("f() ( python3 ./evil.py ); f", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_posix_function_with_a_trailing_redirect_routes_its_body() {
    assert_eq!(
        route("f() { python3 ./evil.py; } >log; f", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_function_keyword_definition_with_a_subshell_body_routes_its_body() {
    assert_eq!(
        route("function f ( python3 ./evil.py ); f", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_function_keyword_definition_with_a_trailing_redirect_routes_its_body() {
    assert_eq!(
        route("function f { python3 ./evil.py; } 2>/dev/null; f", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_posix_function_with_a_case_body_routes_its_body() {
    assert_eq!(
        route("f() case x in x) python3 ./evil.py;; esac; f", &[]),
        vec![evil_py_script_file()]
    );
}

// ── benign variants stay auto-approved ──────────────────────────────────────

#[test]
fn a_benign_brace_group_with_a_trailing_redirect_stays_unrouted() {
    assert_eq!(route("{ echo ok; } 2>/dev/null", &[]), Vec::new());
}

#[test]
fn a_benign_posix_function_with_a_trailing_redirect_stays_unrouted() {
    assert_eq!(route("f() { echo ok; } >log; f", &[]), Vec::new());
}

#[test]
fn a_benign_unnamed_coproc_with_a_trailing_redirect_stays_unrouted() {
    assert_eq!(route("coproc { echo ok; } >log", &[]), Vec::new());
}
