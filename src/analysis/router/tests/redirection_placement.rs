//! Router unit tests pinning that a redirection between assignments,
//! launcher words, and the program does not hide the program from routing.
//! Split from `router::tests` to stay under the file-size budget.

use super::*;

fn evil_py_script_file() -> RoutedTarget {
    RoutedTarget::ScriptFile {
        language: SourceLanguage::Python,
        path: PathBuf::from("./evil.py"),
    }
}

#[test]
fn a_leading_clobber_redirection_does_not_hide_the_program() {
    assert_eq!(
        route(">|out python3 ./evil.py", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_leading_fd_clobber_redirection_does_not_hide_the_program() {
    assert_eq!(
        route("1>|out python3 ./evil.py", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_leading_named_fd_redirection_does_not_hide_the_program() {
    assert_eq!(
        route("{fd}>out python3 ./evil.py", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_redirection_between_an_assignment_and_the_program_does_not_hide_it() {
    assert_eq!(
        route("FOO=1 >out python3 ./evil.py", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_redirection_between_the_env_launcher_and_the_program_does_not_hide_it() {
    assert_eq!(
        route("env >out python3 ./evil.py", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_redirection_between_the_command_launcher_and_the_program_does_not_hide_it() {
    assert_eq!(
        route("command >out python3 ./evil.py", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_benign_command_with_an_assignment_and_a_redirection_stays_unrouted() {
    assert_eq!(route("FOO=1 >out echo ok", &[]), Vec::new());
}
