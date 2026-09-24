//! Router unit tests for `env` launcher shapes wider than the narrow
//! three-token `env -S "<cmd>"` recognition: leading flags before `-S`, a
//! glued `-S<value>`, `--split-string=<value>`, and `-C`/`--chdir` cwd
//! degradation (issue #384).

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
fn env_split_string_single_word_value_with_a_trailing_arg_routes_the_split_command() {
    // GNU `env -S STRING ARGS...` splits STRING into the program's argv and
    // appends any further ARGS to it — it does not require STRING to spell
    // out the whole command line by itself (issue #384/#430).
    assert_eq!(
        route(r#"env -S "python3" ./evil.py"#, &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn env_split_string_value_carrying_its_own_flags_routes_the_split_command() {
    assert_eq!(
        route(r#"env -S "python3 -u" ./evil.py"#, &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn env_split_string_multi_word_value_with_a_trailing_arg_routes_the_split_command() {
    assert_eq!(
        route(r#"env -S "python3 ./evil.py" extra"#, &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn env_split_string_after_chdir_routes_the_split_command_with_a_degraded_cwd() {
    // `-C`/`--chdir` ahead of `-S` must not desync the split-string
    // recognizer into an out-of-range program index that silently drops the
    // whole command (issue #384/#430) — it still routes, with the cwd
    // degradation `-C` itself carries.
    assert_eq!(
        route(r#"env -C d1 -S "python3 ./evil.py""#, &[]),
        vec![evil_py_dynamic()]
    );
}

#[test]
fn env_split_string_of_a_non_interpreter_program_stays_unrouted() {
    assert_eq!(route(r#"env -S "echo ok""#, &[]), Vec::new());
}

#[test]
fn env_split_string_glued_value_of_a_non_interpreter_program_stays_unrouted() {
    assert_eq!(route(r#"env -S "echo" ok"#, &[]), Vec::new());
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

#[test]
fn env_chdir_glued_short_flag_degrades_the_relative_target() {
    // `-Cd1` (no space between the flag and its directory) is the same
    // chdir as the spaced `-C d1` form (#437 review comment 4091038686).
    assert_eq!(
        route("env -Cd1 python3 ./evil.py", &[]),
        vec![evil_py_dynamic()]
    );
}

#[test]
fn env_chdir_behind_a_command_launcher_degrades_the_relative_target() {
    // `command` wraps `env` here, so the `-C`-carrying `env` word is no
    // longer `prefix[0]` — the whole consumed launcher prefix must be
    // scanned for it, not just its first token, or the resolved target
    // gets resolved against the wrong cwd (#437 review comment 4091038686).
    assert_eq!(
        route("command env -C d1 python3 ./evil.py", &[]),
        vec![evil_py_dynamic()]
    );
}
