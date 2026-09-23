//! Router unit tests for an interpreter fed its script through a stdin
//! redirect — glued, fd-prefixed, or placed ahead of the program token
//! (issue #384 B5). A spaced trailing `python3 < ./evil.py` already routed;
//! these pin the shapes that did not.

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
