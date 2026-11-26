use super::*;

mod argv_walk;
mod direct;
mod dynamic_args;
mod dynamic_program;
mod executor_config;
mod runner;
mod unclaimed;
mod wrappers;
use argv_walk::{
    ArgvWalk, bare_stage_interpreter, env_chdir_prefix, leading_stdin_redirect_target,
    walk_interpreter_argv,
};
use direct::route_direct_stage;
use dynamic_args::{dynamic_pipeline_input, dynamic_stage_net, xargs_pipeline_consumer};
use dynamic_program::{alias_value, is_dynamic_program_word};
use executor_config::{
    assignment_stage_executor_routes, env_prefix_executor_route, option_value_executor_route,
    skip_assignment_keyword,
};
use runner::{
    bare_python_script_route, opaque_runner_before_run, opaque_runner_option,
    package_executable_uncertain,
};
use unclaimed::{contains_unquoted_ansi_c_quote, unclaimed_interpreter_net};
use wrappers::{posix_function_definition_body, strip_trailing_redirection, wrapper_bodies};

const MAX_WRAP_DEPTH: u32 = crate::analysis::OrchestrationBudget::L1_DEFAULT.max_depth;

pub(super) fn command_has_heredoc(command: &str) -> bool {
    command.contains("<<") && !aegis_parser::extract_heredoc_bodies(command).is_empty()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CwdState {
    Unset,
    Literal(PathBuf),
    Degraded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HomeState {
    Trusted,
    Degraded,
}

pub(super) fn token_is_home_assignment(tok: &str) -> bool {
    tok == "HOME" || tok.starts_with("HOME=")
}

fn effective_stage_slice<'a>(
    owned_tokens: &'a [String],
) -> (
    Vec<&'a str>,
    Option<aegis_parser::EffectiveTokenSlice<'a>>,
    bool,
) {
    let tokens: Vec<&str> = owned_tokens.iter().map(String::as_str).collect();
    let (slices, truncated) = aegis_parser::effective_token_slices_checked(&tokens);
    let slice = slices.into_iter().next();
    (tokens, slice, truncated)
}

fn parse_cd_like(stage_raw: &str) -> Option<CwdState> {
    let owned_tokens = aegis_parser::split_tokens(stage_raw);
    let (_tokens, slice, _truncated) = effective_stage_slice(&owned_tokens);
    let slice = slice?;

    match slice.program {
        "cd" => Some(match slice.tokens.get(1..) {
            Some([dashdash, path]) if *dashdash == "--" && is_literal_path(path) => {
                CwdState::Literal(PathBuf::from(*path))
            }
            _ => CwdState::Degraded,
        }),
        "pushd" | "popd" | "source" | "." | "eval" => Some(CwdState::Degraded),
        _ => None,
    }
}

fn is_shell_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_assignment_token(tok: &str) -> bool {
    let Some((lhs, _value)) = tok.split_once('=') else {
        return false;
    };
    let lhs = lhs.strip_suffix('+').unwrap_or(lhs);
    let name = match lhs.split_once('[') {
        Some((name, rest)) if rest.ends_with(']') => name,
        Some(_) => return false,
        None => lhs,
    };
    is_shell_identifier(name)
}

pub(super) fn is_assignment_only_stage(tokens: &[&str]) -> bool {
    let mut saw_assignment = false;
    let mut index = 0;
    while index < tokens.len() {
        let tok = tokens[index];
        if aegis_parser::starts_with_redirection_glyph(tok) {
            index += if aegis_parser::is_redirection_operator(tok) {
                2
            } else {
                1
            };
            continue;
        }
        if !is_assignment_token(tok) {
            return false;
        }
        saw_assignment = true;
        index += 1;
    }
    saw_assignment
}

const HOME_OPAQUE_PROGRAMS: &[&str] = &[
    "read",
    "mapfile",
    "readarray",
    "getopts",
    "declare",
    "typeset",
    "local",
    "export",
    "readonly",
    "unset",
    "let",
    "eval",
    "source",
    ".",
];

pub(super) fn is_for_or_select_header(raw_tokens: &[&str]) -> bool {
    matches!(raw_tokens.first(), Some(&"for") | Some(&"select"))
}

fn stage_degrades_home_trust(stage_raw: &str) -> bool {
    let owned_tokens = aegis_parser::split_tokens(stage_raw);
    let (raw_tokens, slice, _truncated) = effective_stage_slice(&owned_tokens);

    if is_for_or_select_header(&raw_tokens) {
        return true;
    }

    if let Some(slice) = &slice {
        if slice.program == "printf" {
            return slice.tokens[1..].iter().any(|tok| tok.starts_with("-v"));
        }
        if HOME_OPAQUE_PROGRAMS.contains(&slice.program) {
            return true;
        }
    }

    is_assignment_only_stage(&raw_tokens)
}

fn fold_cd(
    current: &CwdState,
    effect: CwdState,
    separator: Option<aegis_parser::ListSeparator>,
) -> CwdState {
    if separator != Some(aegis_parser::ListSeparator::And) {
        return CwdState::Degraded;
    }
    match (current, effect) {
        (CwdState::Degraded, _) => CwdState::Degraded,
        (_, CwdState::Literal(new_path)) if new_path.is_absolute() => CwdState::Literal(new_path),
        (CwdState::Literal(base), CwdState::Literal(new_path)) => {
            CwdState::Literal(base.join(new_path))
        }
        (CwdState::Unset, CwdState::Literal(new_path)) => CwdState::Literal(new_path),
        _ => CwdState::Degraded,
    }
}

fn advance_across_separator(
    state: CwdState,
    separator: Option<aegis_parser::ListSeparator>,
) -> CwdState {
    match state {
        CwdState::Literal(_) if separator != Some(aegis_parser::ListSeparator::And) => {
            CwdState::Degraded
        }
        other => other,
    }
}

fn apply_cwd(target: RoutedTarget, cwd: &CwdState) -> RoutedTarget {
    match (target, cwd) {
        (t, CwdState::Unset) => t,
        (RoutedTarget::ScriptFile { language, path }, CwdState::Literal(base))
            if path.is_relative() =>
        {
            RoutedTarget::ScriptFile {
                language,
                path: base.join(path),
            }
        }
        (RoutedTarget::ScriptFile { language, path }, CwdState::Degraded) if path.is_relative() => {
            RoutedTarget::Dynamic {
                language,
                reason: DegradationReason::DynamicSource,
            }
        }
        (RoutedTarget::DirectExec { path }, CwdState::Literal(base)) if path.is_relative() => {
            RoutedTarget::DirectExec {
                path: base.join(path),
            }
        }
        (RoutedTarget::DirectExec { path }, CwdState::Degraded) if path.is_relative() => {
            RoutedTarget::Unresolved {
                reason: DegradationReason::DynamicSource,
            }
        }
        (RoutedTarget::LauncherOperand { path }, CwdState::Literal(base)) if path.is_relative() => {
            RoutedTarget::LauncherOperand {
                path: base.join(path),
            }
        }
        (RoutedTarget::LauncherOperand { path }, CwdState::Degraded) if path.is_relative() => {
            RoutedTarget::Unresolved {
                reason: DegradationReason::DynamicSource,
            }
        }
        (other, _) => other,
    }
}

pub(super) fn route_list_segment(
    segment: &aegis_parser::ListSegment,
    ctx: &RouteContext<'_>,
    cwd: &mut CwdState,
    home: &mut HomeState,
    targets: &mut Vec<RoutedTarget>,
    depth: u32,
) {
    let stages = &segment.pipeline.segments;

    if stages.len() == 1 {
        if stage_degrades_home_trust(&stages[0].raw) {
            *home = HomeState::Degraded;
        }
        if let Some(effect) = parse_cd_like(&stages[0].raw) {
            if let Some(target) = dynamic_stage_net(&stages[0].raw, ctx) {
                push_unique(targets, target);
            }
            *cwd = fold_cd(cwd, effect, segment.separator);
            return;
        }

        route_stage(&stages[0].raw, ctx, cwd, home, targets, depth);
        let current = std::mem::replace(cwd, CwdState::Unset);
        *cwd = advance_across_separator(current, segment.separator);
        return;
    }

    let mut pipeline_had_cd = false;
    let mut stage_targets = Vec::new();
    let mut wrapped_stage_targets = Vec::new();
    for (index, stage) in stages.iter().enumerate() {
        if index > 0
            && xargs_pipeline_consumer(&stage.raw)
            && dynamic_pipeline_input(&stages[index - 1].raw)
        {
            push_unique(
                &mut stage_targets,
                RoutedTarget::Unresolved {
                    reason: DegradationReason::DynamicSource,
                },
            );
        }
        let dynamic_target = dynamic_stage_net(&stage.raw, ctx);
        if let Some(target) = dynamic_target {
            push_unique(&mut stage_targets, target);
        }
        if stage_degrades_home_trust(&stage.raw) {
            *home = HomeState::Degraded;
        }
        if parse_cd_like(&stage.raw).is_some() {
            pipeline_had_cd = true;
            continue;
        }

        let mut scratch_cwd = cwd.clone();
        let mut scratch_home = *home;
        let wrapped_before = wrapped_stage_targets.len();
        route_wrapped_stage(
            &stage.raw,
            ctx,
            &mut scratch_cwd,
            &mut scratch_home,
            &mut wrapped_stage_targets,
            depth,
        );
        let wrapped_produced = wrapped_stage_targets.len() > wrapped_before;
        if scratch_cwd != *cwd {
            pipeline_had_cd = true;
        }
        if scratch_home == HomeState::Degraded {
            *home = HomeState::Degraded;
        }

        let routed = route_direct_stage(&stage.raw, ctx.trusted_aliases);
        if !routed.is_empty() {
            stage_targets.extend(routed);
            continue;
        }
        if wrapped_produced {
            continue;
        }
        if index > 0
            && let Some(bare_interp) = bare_stage_interpreter(&stage.raw, ctx.trusted_aliases)
        {
            if index == 1
                && stages.len() == 2
                && let Some(literal) = printf_percent_s_literal(&stages[0].raw)
            {
                stage_targets.push(RoutedTarget::Inline {
                    language: bare_interp.language,
                    source: literal,
                });
            } else {
                stage_targets.push(RoutedTarget::Dynamic {
                    language: bare_interp.language,
                    reason: DegradationReason::DynamicSource,
                });
            }
            continue;
        }
        stage_targets.extend(unclaimed_interpreter_net(&stage.raw, ctx, *home));
    }

    if pipeline_had_cd {
        *cwd = CwdState::Degraded;
    }
    for target in stage_targets {
        push_unique(targets, apply_cwd(target, cwd));
    }
    for target in wrapped_stage_targets {
        push_unique(targets, target);
    }
    let current = std::mem::replace(cwd, CwdState::Unset);
    *cwd = advance_across_separator(current, segment.separator);
}

fn push_unique(targets: &mut Vec<RoutedTarget>, target: RoutedTarget) {
    if !targets.contains(&target) {
        targets.push(target);
    }
}

fn route_stage(
    stage_raw: &str,
    ctx: &RouteContext<'_>,
    cwd: &mut CwdState,
    home: &mut HomeState,
    targets: &mut Vec<RoutedTarget>,
    depth: u32,
) {
    let dynamic_target = dynamic_stage_net(stage_raw, ctx);
    if let Some(target) = dynamic_target {
        push_unique(targets, target);
    }
    let direct = route_direct_stage(stage_raw, ctx.trusted_aliases);
    let mut claimed = !direct.is_empty();
    for target in direct {
        push_unique(targets, apply_cwd(target, cwd));
    }

    let mut wrapped_targets = Vec::new();
    route_wrapped_stage(stage_raw, ctx, cwd, home, &mut wrapped_targets, depth);
    claimed |= !wrapped_targets.is_empty();
    for target in wrapped_targets {
        push_unique(targets, target);
    }

    if !claimed {
        for net_target in unclaimed_interpreter_net(stage_raw, ctx, *home) {
            push_unique(targets, apply_cwd(net_target, cwd));
        }
    }
}

const WRAPPER_ONLY_RESERVED_WORDS: &[&str] = &["case", "function", "coproc"];

fn stage_starts_with_reserved_word(trimmed: &str, word: &str) -> bool {
    trimmed
        .strip_prefix(word)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
}

fn stage_may_be_wrapped(stage_raw: &str) -> bool {
    let trimmed = stage_raw.trim_start();
    trimmed.starts_with('(')
        || trimmed.starts_with('{')
        || stage_raw.contains("$(")
        || stage_raw.contains('`')
        || stage_raw.contains("<(")
        || stage_raw.contains(">(")
        || aegis_parser::COMMAND_STARTING_KEYWORDS
            .iter()
            .chain(WRAPPER_ONLY_RESERVED_WORDS)
            .any(|kw| stage_starts_with_reserved_word(trimmed, kw))
        || (command_has_heredoc(stage_raw) && heredoc_marker_line_tail(stage_raw).is_some())
        || posix_function_definition_body(trimmed).is_some()
}

fn heredoc_marker_line_tail(stage_raw: &str) -> Option<&str> {
    let first_line = stage_raw.lines().next()?;
    let (_, mut after) = aegis_parser::split_at_heredoc_marker(first_line)?;
    while let Some((_, next_after)) = aegis_parser::split_at_heredoc_marker(after) {
        after = next_after;
    }
    let trimmed = after.trim_start();
    let without_operator = trimmed
        .strip_prefix("&&")
        .or_else(|| trimmed.strip_prefix("||"))
        .or_else(|| trimmed.strip_prefix(';'))
        .unwrap_or(trimmed)
        .trim_start();
    (!without_operator.is_empty()).then_some(without_operator)
}

fn route_wrapped_stage(
    stage_raw: &str,
    ctx: &RouteContext<'_>,
    cwd: &mut CwdState,
    home: &mut HomeState,
    targets: &mut Vec<RoutedTarget>,
    depth: u32,
) {
    if !stage_may_be_wrapped(stage_raw) {
        return;
    }

    if depth >= MAX_WRAP_DEPTH {
        push_unique(
            targets,
            RoutedTarget::Unresolved {
                reason: DegradationReason::LimitExceeded,
            },
        );
        return;
    }

    for body in wrapper_bodies(stage_raw, ctx.trusted_aliases) {
        let mut body_cwd = cwd.clone();
        let mut body_home = *home;
        let mut body_targets = Vec::new();
        for segment in aegis_parser::list_segments(&body) {
            route_list_segment(
                &segment,
                ctx,
                &mut body_cwd,
                &mut body_home,
                &mut body_targets,
                depth + 1,
            );
        }
        for target in body_targets {
            push_unique(targets, target);
        }
        if body_cwd != *cwd {
            *cwd = CwdState::Degraded;
        }
        if body_home == HomeState::Degraded {
            *home = HomeState::Degraded;
        }
    }
}
