use super::*;

/// Route a single pipeline stage without looking through grammar wrappers (#421, ADR-040).
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
    let (tokens, slice, truncated) = effective_stage_slice(&owned_tokens);
    if truncated {
        // Fail closed when `env -S` nesting hits the parser's depth bound (`CONVENTION.md` §2).
        return vec![RoutedTarget::Unresolved {
            reason: DegradationReason::LimitExceeded,
        }];
    }
    let Some(slice) = slice else {
        return Vec::new();
    };

    // Options before a runner's `run` subcommand can alter cwd/config (#421, ADR-040).
    if opaque_runner_before_run(&slice.tokens) {
        return vec![RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        }];
    }

    // `env -S` can expand into more tokens than `tokens`; fail closed if the prefix index cannot be recovered (`CONVENTION.md` §2).
    let Some(effective_start) = tokens.len().checked_sub(slice.tokens.len()) else {
        return vec![RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        }];
    };
    // `env -C`/`--chdir` changes only the child cwd, so relative targets degrade (#384).
    let env_cwd = if env_chdir_prefix(&tokens[..effective_start]) {
        CwdState::Degraded
    } else {
        CwdState::Unset
    };

    // Runner option hides the real child argv (#421, ADR-040).
    if opaque_runner_option(&tokens[..effective_start], slice.program) {
        return vec![RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        }];
    }

    // `uv run`/`pipx run` execute bare `.py` operands as Python without a shebang (#421, ADR-040).
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

/// Package-selecting runners pick their own child binary, so a visible target still degrades (#421, ADR-040).
fn degrade_package_uncertain(mut routed: Vec<RoutedTarget>, prefix: &[&str]) -> Vec<RoutedTarget> {
    if !routed.is_empty() && package_executable_uncertain(prefix) {
        routed.push(RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        });
    }
    routed
}
