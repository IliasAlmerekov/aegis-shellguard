use super::*;
use aegis_parser::Runner;

/// `true` when some token in `prefix` is a recognized runner program whose
/// own prefix (starting at that token) satisfies `predicate`. Every
/// runner-prefix scan in this module shares this shape — differing only in
/// what `predicate` checks about the matched runner and its own trailing
/// tokens.
fn any_runner_prefix(prefix: &[&str], predicate: impl Fn(Runner, &[&str]) -> bool) -> bool {
    prefix.iter().enumerate().any(|(index, token)| {
        Runner::from_program(token).is_some_and(|runner| predicate(runner, &prefix[index..]))
    })
}

pub(super) fn opaque_runner_option(prefix: &[&str], program: &str) -> bool {
    if !program.starts_with('-') {
        return false;
    }
    any_runner_prefix(prefix, |runner, rest| {
        runner.command_prefix_len(rest).is_some()
    })
}

pub(super) fn opaque_runner_before_run(tokens: &[&str]) -> bool {
    tokens
        .first()
        .is_some_and(|token| Runner::from_program(token).is_some_and(Runner::has_run_subcommand))
        && tokens.get(1).is_some_and(|token| token.starts_with('-'))
        && tokens[2..].contains(&"run")
}

pub(super) fn package_executable_uncertain(prefix: &[&str]) -> bool {
    any_runner_prefix(prefix, |runner, rest| {
        runner.selects_package_executable(rest)
    })
}

pub(super) fn bare_python_script_route(prefix: &[&str], program: &str) -> Option<RoutedTarget> {
    let runs_python_script =
        any_runner_prefix(prefix, |runner, rest| runner.runs_bare_python_script(rest));
    let forces_python_script =
        any_runner_prefix(prefix, |runner, rest| runner.forces_python_script(rest));
    (runs_python_script && (program.ends_with(".py") || forces_python_script)).then(|| {
        RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from(program),
        }
    })
}
