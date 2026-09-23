//! Router unit tests for `env` launcher shapes wider than the narrow
//! three-token `env -S "<cmd>"` recognition: leading flags before `-S`, a
//! glued `-S<value>`, `--split-string=<value>`, and `-C`/`--chdir` cwd
//! degradation (issue #384 B4).

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
fn env_split_string_glued_flag_routes_the_split_command() {
    assert_eq!(
        route(r#"env -S"python3 ./evil.py""#, &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn env_split_string_after_ignore_environment_routes_the_split_command() {
    assert_eq!(
        route(r#"env -i -S "python3 ./evil.py""#, &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn env_split_string_after_unset_routes_the_split_command() {
    assert_eq!(
        route(r#"env -u X -S "python3 ./evil.py""#, &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn env_split_string_long_flag_equals_form_routes_the_split_command() {
    assert_eq!(
        route(r#"env --split-string="python3 ./evil.py""#, &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn env_chdir_short_flag_degrades_the_relative_target() {
    assert_eq!(
        route("env -C d1 python3 ./evil.py", &[]),
        vec![evil_py_dynamic()]
    );
}

#[test]
fn env_chdir_long_flag_equals_form_degrades_the_relative_target() {
    assert_eq!(
        route("env --chdir=d1 python3 ./evil.py", &[]),
        vec![evil_py_dynamic()]
    );
}

#[test]
fn env_chdir_long_flag_spaced_form_degrades_the_relative_target() {
    assert_eq!(
        route("env --chdir d1 python3 ./evil.py", &[]),
        vec![evil_py_dynamic()]
    );
}

#[test]
fn env_assignment_only_stays_unrouted() {
    assert_eq!(route("env FOO=1 cargo test", &[]), Vec::new());
}

#[test]
fn env_ignore_environment_with_assignment_stays_unrouted() {
    assert_eq!(route("env -i PATH=/usr/bin ls", &[]), Vec::new());
}

#[test]
fn env_chdir_before_a_non_interpreter_program_stays_unrouted() {
    assert_eq!(route("env -C d1 ls", &[]), Vec::new());
}
