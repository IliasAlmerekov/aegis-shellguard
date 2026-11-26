use super::*;

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
        return vec![RoutedTarget::Unresolved {
            reason: DegradationReason::LimitExceeded,
        }];
    }
    let Some(slice) = slice else {
        return Vec::new();
    };

    if opaque_runner_before_run(&slice.tokens) {
        return vec![RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        }];
    }

    let Some(effective_start) = tokens.len().checked_sub(slice.tokens.len()) else {
        return vec![RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        }];
    };
    let env_cwd = if env_chdir_prefix(&tokens[..effective_start]) {
        CwdState::Degraded
    } else {
        CwdState::Unset
    };

    if opaque_runner_option(&tokens[..effective_start], slice.program) {
        return vec![RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        }];
    }

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

fn degrade_package_uncertain(mut routed: Vec<RoutedTarget>, prefix: &[&str]) -> Vec<RoutedTarget> {
    if !routed.is_empty() && package_executable_uncertain(prefix) {
        routed.push(RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        });
    }
    routed
}
