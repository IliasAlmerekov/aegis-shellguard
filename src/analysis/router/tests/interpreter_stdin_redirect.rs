//! Router unit tests for an interpreter fed its script through a stdin
//! redirect — glued, fd-prefixed, or placed ahead of the program token
//! (issue #384). Covers the shapes beyond a spaced trailing
//! `python3 < ./evil.py`.

use super::*;

fn evil_py_script_file() -> RoutedTarget {
    RoutedTarget::ScriptFile {
        language: SourceLanguage::Python,
        path: PathBuf::from("./evil.py"),
    }
}

fn evil_py_dynamic() -> RoutedTarget {
    RoutedTarget::Dynamic {
        language: SourceLanguage::Python,
        reason: DegradationReason::DynamicSource,
    }
}

#[test]
fn a_glued_trailing_stdin_redirect_routes_the_file() {
    assert_eq!(
        route("python3 <./evil.py", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn an_fd_prefixed_glued_trailing_stdin_redirect_routes_the_file() {
    assert_eq!(
        route("python3 0<./evil.py", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_glued_trailing_stdin_redirect_routes_a_shell_script() {
    assert_eq!(
        route("bash <./x.sh", &[]),
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Bash,
            path: PathBuf::from("./x.sh"),
        }]
    );
}

#[test]
fn a_glued_trailing_stdin_redirect_routes_a_javascript_file() {
    assert_eq!(
        route("node <./evil.js", &[]),
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::JavaScript,
            path: PathBuf::from("./evil.js"),
        }]
    );
}

#[test]
fn a_glued_leading_stdin_redirect_routes_the_file() {
    assert_eq!(
        route("<./evil.py python3", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_spaced_leading_stdin_redirect_routes_the_file() {
    assert_eq!(
        route("< ./evil.py python3", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn multiple_leading_stdin_redirects_use_the_last_one() {
    // Shell redirections apply left to right, so the second `<` here wins
    // and becomes the interpreter's actual stdin (#437 review comment
    // 4091038705) — matching the overwrite behavior already used for
    // redirects placed after the program.
    assert_eq!(
        route("< ./benign.py < ./evil.py python3", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_pipe_into_an_interpreter_with_a_bare_stdin_sentinel_degrades_dynamically() {
    // `-` alone tells a real interpreter to read its script from stdin, same
    // as no argument at all — the piped-in `evil.py` content, not nothing
    // (#437 review comment 4091038647).
    assert_eq!(
        route("cat ./evil.py | python3 -", &[]),
        vec![evil_py_dynamic()]
    );
}

#[test]
fn a_pipe_into_an_interpreter_with_flags_before_the_stdin_sentinel_degrades_dynamically() {
    assert_eq!(
        route("cat ./evil.py | python3 -u -", &[]),
        vec![evil_py_dynamic()]
    );
}

#[test]
fn a_dynamic_here_string_command_substitution_degrades() {
    assert_eq!(
        route(r#"python3 <<<"$(cat ./evil.py)""#, &[]),
        vec![evil_py_dynamic()]
    );
}

#[test]
fn a_dynamic_here_string_variable_expansion_degrades() {
    assert_eq!(route(r#"python3 <<<"$X""#, &[]), vec![evil_py_dynamic()]);
}

#[test]
fn a_literal_here_string_stays_routed_as_inline_source() {
    assert_eq!(
        route(r#"python3 <<<"print(1)""#, &[]),
        vec![RoutedTarget::Inline {
            language: SourceLanguage::Python,
            source: "print(1)".to_owned(),
        }]
    );
}
