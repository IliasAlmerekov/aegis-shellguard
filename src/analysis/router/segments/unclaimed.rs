use super::executor_value::{
    basename, command_value_leading_word, d4_value, route_path_like_candidate,
    token_names_an_interpreter,
};
use super::*;
use std::ops::ControlFlow;

const NAME_ONLY_PROGRAMS: &[&str] = &[
    "echo",
    "printf",
    "which",
    "type",
    "whereis",
    "man",
    "info",
    "help",
    "apropos",
    "grep",
    "egrep",
    "fgrep",
    "rg",
    "ag",
    "ls",
    "cat",
    "head",
    "tail",
    "less",
    "more",
    "file",
    "stat",
    "wc",
    "diff",
    "apt",
    "apt-get",
    "apt-cache",
    "dnf",
    "yum",
    "brew",
    "pacman",
    "git",
    "update-alternatives",
    "dpkg",
    "mkdir",
    "rmdir",
    "touch",
    "rm",
    "cp",
    "mv",
    "ln",
    "chmod",
    "chown",
    "chgrp",
    "basename",
    "dirname",
    "realpath",
    "readlink",
];

fn is_command_lookup(raw_tokens: &[&str]) -> bool {
    let Some(first) = raw_tokens.first() else {
        return false;
    };
    basename(first) == "command" && matches!(raw_tokens.get(1), Some(&"-v") | Some(&"-V"))
}

pub(super) fn contains_unquoted_ansi_c_quote(text: &str) -> bool {
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut chars = text.char_indices();
    while let Some((idx, ch)) = chars.next() {
        match ch {
            '\\' if !in_single_quote => {
                chars.next();
            }
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            '$' if !in_single_quote
                && !in_double_quote
                && text[idx + ch.len_utf8()..].starts_with('\'') =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}

fn is_pure_assignment_stage(raw_tokens: &[&str]) -> bool {
    is_assignment_only_stage(skip_assignment_keyword(raw_tokens))
}

pub(super) fn unclaimed_interpreter_net(
    stage_raw: &str,
    ctx: &RouteContext<'_>,
    home: HomeState,
) -> Vec<RoutedTarget> {
    let masked_stage_raw;
    let stage_raw = if stage_raw.contains("<<") {
        masked_stage_raw = aegis_parser::mask_inert_heredoc_substitution_markers(stage_raw);
        masked_stage_raw.as_str()
    } else {
        stage_raw
    };

    let stage_text = strip_trailing_redirection(stage_raw);
    let owned_tokens = aegis_parser::split_tokens(stage_text);
    let raw_tokens: Vec<&str> = owned_tokens.iter().map(String::as_str).collect();
    if is_command_lookup(&raw_tokens) {
        return Vec::new();
    }

    let mut targets = assignment_stage_executor_routes(&raw_tokens, home, ctx);
    if !targets.is_empty() {
        return targets;
    }

    if is_pure_assignment_stage(&raw_tokens) {
        return Vec::new();
    }

    let Some(slice) = aegis_parser::effective_token_slices(&raw_tokens)
        .into_iter()
        .next()
    else {
        return Vec::new();
    };

    let operands = slice.tokens[1..].iter().filter(|tok| !tok.starts_with('-'));
    if operands.clone().next().is_some() {
        if is_dynamic_program_word(slice.program) {
            return vec![RoutedTarget::Unresolved {
                reason: DegradationReason::DynamicSource,
            }];
        }
        let alias_scope = ctx
            .command
            .get(..ctx.alias_scope_end.get())
            .unwrap_or(ctx.command);
        if let Some(value) = alias_value(alias_scope, slice.program) {
            let replacement = value.trim();
            if replacement.is_empty()
                || is_dynamic_program_word(replacement)
                || replacement
                    .split_whitespace()
                    .any(|word| token_names_an_interpreter(word, ctx.trusted_aliases))
            {
                return vec![RoutedTarget::Unresolved {
                    reason: DegradationReason::DynamicSource,
                }];
            }
        }
    }

    let (option_routes, option_consumed) =
        option_value_executor_route(slice.program, &raw_tokens, &slice.tokens, home, ctx);
    for route in env_prefix_executor_route(&raw_tokens, home, ctx)
        .into_iter()
        .chain(option_routes)
    {
        if matches!(route, RoutedTarget::Unresolved { .. }) {
            return vec![route];
        }
        targets.push(route);
    }

    if NAME_ONLY_PROGRAMS.contains(&slice.program) {
        return targets;
    }

    if contains_unquoted_ansi_c_quote(stage_text) {
        return vec![RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        }];
    }

    let names_an_interpreter = slice.tokens[1..]
        .iter()
        .any(|tok| token_names_an_interpreter(tok, ctx.trusted_aliases));
    if names_an_interpreter {
        return vec![RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        }];
    }

    let masked_tokens = match aegis_parser::mask_command_substitutions(stage_text) {
        Some(std::borrow::Cow::Owned(masked)) => aegis_parser::split_tokens(&masked),
        _ => Vec::new(),
    };
    let masked_refs: Vec<&str> = masked_tokens.iter().map(String::as_str).collect();
    let masked_slice = aegis_parser::effective_token_slices(&masked_refs)
        .into_iter()
        .next();
    let candidate_tokens = masked_slice
        .as_ref()
        .map_or(&slice.tokens[1..], |masked| &masked.tokens[1..]);

    let mut seen = std::collections::HashSet::new();
    for token in candidate_tokens {
        if option_consumed.contains(token) {
            continue;
        }
        if token_is_home_assignment(token) {
            return vec![RoutedTarget::Unresolved {
                reason: DegradationReason::DynamicSource,
            }];
        }
        if let Some(value) = d4_value(token) {
            if let ControlFlow::Break(degraded) =
                record_path_candidate(value, &mut seen, home, ctx, &mut targets)
            {
                return degraded;
            }
            continue;
        }
        if !token.starts_with('-') {
            if let ControlFlow::Break(degraded) =
                record_path_candidate(token, &mut seen, home, ctx, &mut targets)
            {
                return degraded;
            }
            continue;
        }
        if token.starts_with("--") {
            continue;
        }
        for suffix in glued_short_flag_values(token) {
            if suffix.contains(char::is_whitespace)
                && token_names_an_interpreter(suffix, ctx.trusted_aliases)
            {
                return vec![RoutedTarget::Unresolved {
                    reason: DegradationReason::DynamicSource,
                }];
            }
            if let ControlFlow::Break(degraded) =
                record_path_candidate(suffix, &mut seen, home, ctx, &mut targets)
            {
                return degraded;
            }
        }
    }
    targets
}

enum CandidateOutcome {
    Skip,
    Degrade(RoutedTarget),
    Add(RoutedTarget),
}

fn evaluate_path_candidate<'a>(
    candidate: &'a str,
    seen: &mut std::collections::HashSet<&'a str>,
    home: HomeState,
    ctx: &RouteContext<'_>,
) -> CandidateOutcome {
    if !candidate.contains('/') || candidate.contains("://") {
        return CandidateOutcome::Skip;
    }
    let candidate = command_value_leading_word(candidate);
    if !seen.insert(candidate) {
        return CandidateOutcome::Skip;
    }
    match route_path_like_candidate(candidate, home, ctx) {
        routed @ RoutedTarget::Unresolved { .. } => CandidateOutcome::Degrade(routed),
        routed => CandidateOutcome::Add(routed),
    }
}

fn record_path_candidate<'a>(
    candidate: &'a str,
    seen: &mut std::collections::HashSet<&'a str>,
    home: HomeState,
    ctx: &RouteContext<'_>,
    targets: &mut Vec<RoutedTarget>,
) -> ControlFlow<Vec<RoutedTarget>> {
    match evaluate_path_candidate(candidate, seen, home, ctx) {
        CandidateOutcome::Skip => ControlFlow::Continue(()),
        CandidateOutcome::Degrade(routed) => ControlFlow::Break(vec![routed]),
        CandidateOutcome::Add(routed) => {
            targets.push(routed);
            ControlFlow::Continue(())
        }
    }
}

fn glued_short_flag_values(token: &str) -> Vec<&str> {
    let Some(rest) = token.strip_prefix('-') else {
        return Vec::new();
    };
    let letters = rest.chars().take_while(char::is_ascii_alphabetic).count();
    (1..=letters)
        .map(|split| &rest[split..])
        .filter(|suffix| !suffix.is_empty())
        .collect()
}
