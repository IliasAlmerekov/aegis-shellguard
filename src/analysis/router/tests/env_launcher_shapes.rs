//! Router unit tests for `env` launcher shapes wider than the narrow
//! three-token `env -S "<cmd>"` recognition: leading flags before `-S`, a
//! glued `-S<value>`, `--split-string=<value>`, and `-C`/`--chdir` cwd
//! degradation (issue #384).

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

/// `env_split_string_tokens` re-splits a `-S` value on plain whitespace,
/// with no quote awareness of its own (issue #437 review, PR #437 adversarial
/// finding F3) — a value that quotes its own spaces (`'import os; os.system
/// ("id")'`) re-splits into more words than the original command had tokens
/// to begin with. `route_direct_stage` used to compute the split's start
/// position in the original tokens as `tokens.len() - slice.tokens.len()`,
/// which assumes the split can only ever be as short as, or shorter than,
/// what it replaced; here it is longer, and the subtraction underflowed
/// (`attempt to subtract with overflow` in a debug build, an out-of-range
/// slice index in release). The fix must fail closed instead of computing a
/// bogus index: this exact stage degrades rather than panicking or auto-
/// approving.
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

/// Sweep of `env -S` shapes whose value re-splits into more words than the
/// stage had tokens — nested quoting, embedded semicolons, extra leading
/// assignments, and a doubly-nested `env -S` — asserting only that routing
/// never panics (issue #437 review, PR #437 adversarial finding F3). Each
/// shape is run through `std::panic::catch_unwind` so one failure still
/// reports every other shape's outcome rather than aborting the sweep.
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

/// The exact adversarial shape from #437 review finding F1: `env -S 'env
/// -S' 'env -S' ... 'true'` nested 3000 levels deep, each level re-splitting
/// into the same shape one repeat shorter (`env_split_string_tokens` appends
/// the remaining operands as-is). Before `aegis-parser`'s `ENV_SPLIT_MAX_DEPTH`
/// bound this overflowed the stack; now it must finish quickly and route to
/// `Unresolved { reason: LimitExceeded }` rather than silently falling
/// through to "no program" (which the caller would otherwise read as an
/// ordinary safe/empty stage and auto-approve).
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
