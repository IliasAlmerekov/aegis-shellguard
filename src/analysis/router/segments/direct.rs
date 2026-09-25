use super::*;

/// Resolve `stage` (one pipeline stage's raw text) to its own route without
/// looking through any grammar wrapper: the narrow heredoc-write-then-exec
/// reuse shape first (when `stage` owns a heredoc marker), then a runner
/// option sitting before its own `run` subcommand (`opaque_runner_before_run`),
/// then a runner option the parser landed on as if it were the program
/// (`opaque_runner_option`), then a bare `.py` operand under `uv run`/`pipx
/// run` (`bare_python_script_route`), then explicit interpreter inline/file/
/// redirection argv walk, then heredoc/here-string stdin fallback, then a
/// bare path-like direct-exec candidate — with a trailing
/// `package_executable_uncertain` check appended wherever a runner selects
/// its own child binary instead of naming it (#421, ADR-040).
pub(super) fn route_direct_stage(
    stage: &str,
    trusted_aliases: &[(&str, &str)],
) -> Vec<RoutedTarget> {
    if command_has_heredoc(stage)
        && let Some(targets) = heredoc_write_then_exec_reuse(stage, trusted_aliases)
    {
        return targets;
    }

    let owned_tokens = aegis_parser::split_tokens(stage);
    if owned_tokens.is_empty() {
        return Vec::new();
    }
    // A redirection anywhere before the program (`>out python3 x.py`,
    // `FOO=1 >out python3 x.py`, `env >out python3 x.py`) is shell syntax
    // attached to the stage, not an argument of the program that follows it
    // — the shell strips it before argv0 resolution, so routing must too.
    // `aegis_parser::effective_token_slices` handles this at every step
    // (issue #384), not only a redirection at position 0.
    let (tokens, slice, truncated) = effective_stage_slice(&owned_tokens);
    if truncated {
        // An `env -S`/`--split-string` chain nested past
        // `aegis_parser`'s own bound: some program is still hidden behind
        // an unexamined split, same shape as `route_wrapped_stage`'s
        // `MAX_WRAP_DEPTH` block above. Degrade honestly instead of
        // falling through to the `slice.is_none()` case below, which
        // would otherwise read this exactly like an ordinary stage with
        // no program at all (fail-closed, CONVENTION.md §2, #437 review
        // finding F1).
        return vec![RoutedTarget::Unresolved {
            reason: DegradationReason::LimitExceeded,
        }];
    }
    let Some(slice) = slice else {
        return Vec::new();
    };

    // A runner option sits before its own `run` subcommand (`uv --directory
    // X run ...`): the child program named after `run` cannot be trusted to
    // run under the cwd/config that option implies (#421, ADR-040).
    if opaque_runner_before_run(&slice.tokens) {
        return vec![RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        }];
    }

    // `slice.tokens` is a suffix of `tokens` when it came from an index into
    // `tokens` itself, but an `env -S`/`--split-string` value re-splits on
    // plain whitespace with no quote awareness (`aegis_parser::
    // env_split_string_tokens`) — a value that quotes its own spaces can
    // re-split into more words than the stage had tokens to begin with, so
    // `slice.tokens` is longer than `tokens` and no such suffix index
    // exists. Fail closed rather than let the subtraction underflow (#437
    // review, adversarial finding F3): the stage's own prefix (an `env -C`
    // chdir flag, a leading redirect) cannot be recovered without that
    // index, so treat it exactly as unresolved as any other source routing
    // cannot statically recover (CONVENTION.md §2).
    let Some(effective_start) = tokens.len().checked_sub(slice.tokens.len()) else {
        return vec![RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        }];
    };
    // `env -C DIR`/`--chdir[=]DIR` changes the cwd for that one child
    // process only, not the shell's own — a relative target must degrade
    // rather than resolve against the shell's own cwd (issue #384).
    let env_cwd = if env_chdir_prefix(&tokens[..effective_start]) {
        CwdState::Degraded
    } else {
        CwdState::Unset
    };

    // A recognized runner prefix landed on an option (`npx -c`) instead of a
    // program: the real child argv is hidden behind that option, so it
    // cannot be parsed as a known program (#421, ADR-040).
    if opaque_runner_option(&tokens[..effective_start], slice.program) {
        return vec![RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        }];
    }

    // `uv run`/`pipx run` treat a bare `.py` operand as Python even without
    // a shebang, unlike an ordinary direct-exec candidate (#421, ADR-040).
    if let Some(target) =
        bare_python_script_route(&tokens[..effective_start], tokens[effective_start])
    {
        return vec![apply_cwd(target, &env_cwd)];
    }

    let Some(interp) = resolve_interpreter(slice.program, trusted_aliases) else {
        let routed = direct_exec_route(tokens[effective_start])
            .into_iter()
            .collect();
        let routed = degrade_package_uncertain(routed, &tokens[..effective_start]);
        return routed
            .into_iter()
            .map(|target| apply_cwd(target, &env_cwd))
            .collect();
    };

    let rest = &slice.tokens[1..];
    let routed = match walk_interpreter_argv(interp, rest) {
        ArgvWalk::Routed(target) => vec![target],
        ArgvWalk::NoSource => Vec::new(),
        ArgvWalk::NoMatch => {
            if let Some(stdin_route) =
                heredoc::heredoc_stdin(stage).or_else(|| heredoc::here_string_stdin(rest))
            {
                vec![stdin_target(interp.language, stdin_route)]
            } else if let Some(path) = leading_stdin_redirect_target(&tokens[..effective_start]) {
                vec![RoutedTarget::ScriptFile {
                    language: interp.language,
                    path: PathBuf::from(path),
                }]
            } else {
                Vec::new()
            }
        }
    };
    let routed = degrade_package_uncertain(routed, &tokens[..effective_start]);
    routed
        .into_iter()
        .map(|target| apply_cwd(target, &env_cwd))
        .collect()
}

/// Append a `DynamicSource` degradation to `routed` when `prefix` names a
/// package-selecting runner and routing already found a target: `uvx`/
/// `npx`/`pipx run`/`uv tool run` pick the child binary themselves, so a
/// visible source Match — from an explicit interpreter argv walk or a bare
/// direct-exec operand alike — never rules out a different package-provided
/// executable actually running (#421, ADR-040).
fn degrade_package_uncertain(mut routed: Vec<RoutedTarget>, prefix: &[&str]) -> Vec<RoutedTarget> {
    if !routed.is_empty() && package_executable_uncertain(prefix) {
        routed.push(RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        });
    }
    routed
}
