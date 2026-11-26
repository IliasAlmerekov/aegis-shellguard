use super::*;
use std::time::Instant;

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
    assert_eq!(
        route("env -Cd1 python3 ./evil.py", &[]),
        vec![evil_py_dynamic()]
    );
}

#[test]
fn env_chdir_behind_a_command_launcher_degrades_the_relative_target() {
    assert_eq!(
        route("command env -C d1 python3 ./evil.py", &[]),
        vec![evil_py_dynamic()]
    );
}

#[test]
fn env_split_string_value_that_re_splits_longer_than_the_original_tokens_degrades_instead_of_panicking()
 {
    let stage = "FOO=1 env -S \"python3 -c 'import os; os.system(\\\"id\\\")'\"";
    assert_eq!(
        route(stage, &[]),
        vec![RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource
        }]
    );
}

#[test]
fn env_split_string_re_split_sweep_never_panics() {
    let stages = [
        "FOO=1 env -S \"python3 -c 'import os; os.system(\\\"id\\\")'\"",
        "env -S \"a b c d e f\"",
        "env -S \"python3 -c 'import sys; sys.exit(1)'\"",
        "FOO=1 BAR=2 env -S \"node -e 'console.log(1); console.log(2)'\"",
        "env -S \"env -S 'perl -e \\\"print 1\\\"'\"",
    ];
    let panicked: Vec<&str> = stages
        .into_iter()
        .filter(|stage| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| route(stage, &[]))).is_err()
        })
        .collect();
    assert!(panicked.is_empty(), "these shapes panicked: {panicked:?}");
}

#[test]
fn env_dash_s_chain_past_the_nesting_bound_finishes_quickly_and_fails_closed() {
    let command = format!("env -S {}'true'", "'env -S' ".repeat(3000));

    let start = Instant::now();
    let targets = route(&command, &[]);
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_secs() < 5,
        "3000-level env -S nesting must stay well within a generous bound, took {elapsed:?}"
    );
    assert_eq!(
        targets,
        vec![RoutedTarget::Unresolved {
            reason: DegradationReason::LimitExceeded
        }],
        "past the env -S nesting bound routing must degrade rather than silently \
         treat the unresolved program as safe"
    );
}
