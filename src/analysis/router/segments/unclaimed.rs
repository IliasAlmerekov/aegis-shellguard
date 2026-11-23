//! Fail-closed net for stages no other routing path claims (#384, #430, ADR-022 §6).

use super::*;
use std::ops::ControlFlow;

/// Programs that treat command names as data without executing them.
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

/// Checked on raw tokens because launcher stripping removes `command` before `unclaimed_interpreter_net` sees it.
fn is_command_lookup(raw_tokens: &[&str]) -> bool {
    let Some(first) = raw_tokens.first() else {
        return false;
    };
    basename(first) == "command" && matches!(raw_tokens.get(1), Some(&"-v") | Some(&"-V"))
}

fn basename(token: &str) -> &str {
    token.rsplit('/').next().unwrap_or(token)
}

/// Quote/backslash-aware scan for `$'...'` outside quotes (Rule C, #384, #430).
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

/// Fail-closed net for unclaimed stages: scans executor configs, dynamic/aliased programs,
/// interpreter names, and path-like operands (#384, #430, GHSA-xj54).
pub(super) fn unclaimed_interpreter_net(
    stage_raw: &str,
    ctx: &RouteContext<'_>,
    home: HomeState,
) -> Vec<RoutedTarget> {
    // Mask `Data consumer` heredoc bodies so inert data is not read as argv (#396);
    // `contains("<<")` keeps heredoc-free stages allocation-free (`CONVENTION.md` §8).
    let masked_stage_raw;
    let stage_raw = if stage_raw.contains("<<") {
        masked_stage_raw = aegis_parser::mask_inert_heredoc_substitution_markers(stage_raw);
        masked_stage_raw.as_str()
    } else {
        stage_raw
    };

    // Strip trailing redirections so redirect targets are not read as path operands (#384, #430).
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

    // Pure-assignment stages invoke no program, so non-executor values are not launcher operands (GHSA-xj54).
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
        // Keep conservative program-word fallback; caller deduplicates stage results.
        if is_dynamic_program_word(slice.program) {
            return vec![RoutedTarget::Unresolved {
                reason: DegradationReason::DynamicSource,
            }];
        }
        // Degrade when an earlier `alias` maps this program to an interpreter, dynamic word, or empty text (#384, #430).
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

    // Checked before `NAME_ONLY_PROGRAMS` because some read-only tools accept executor env/options (#384, #430).
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

    // Rule C (#384, #430): degrade stages with unquoted `$'...'` (canonicalized pieces keep a leading `$''`).
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

    // Routed as `LauncherOperand`, not `DirectExec`: the net picked these operands
    // out of an unclaimed stage, so a missing path or directory resolves
    // speculatively instead of degrading (#384, #430).
    // Every distinct path-like operand is a candidate; a flag value can shadow the
    // real script. Only a `/` the shell writes itself counts, so substitutions are
    // masked first.
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
        // Skip separate-token executor values already parsed above (#384, #430).
        if option_consumed.contains(token) {
            continue;
        }
        // Check whole token before `d4_value` strips `HOME=` (GHSA-xj54).
        if token_is_home_assignment(token) {
            return vec![RoutedTarget::Unresolved {
                reason: DegradationReason::DynamicSource,
            }];
        }
        // `prefix=value` tokens candidate on `value` even when `-`-prefixed (#384, #430).
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
        // Short-flag bundles can glue values onto flag letters; test every split (GHSA-xj54).
        // Require whitespace for interpreter degradation so single-word linker/include flags (`-lpython3.12`) stay clean.
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

/// Gate on `/` and non-URL before reducing to the leading word so quoted substitutions still hit `is_literal_path` (GHSA-xj54).
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

/// Non-empty suffixes after 1..=N leading ASCII flag letters of a single-dash token (GHSA-xj54).
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

/// Route a path-like word to `LauncherOperand` or degrade; `~//rest` trims leading `/` so `join` does not drop `home` (GHSA-xj54).
fn route_path_like_candidate(word: &str, home: HomeState, ctx: &RouteContext<'_>) -> RoutedTarget {
    if token_is_home_assignment(word) {
        return RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        };
    }

    let path = if let Some(rest) = word.strip_prefix("~/").map(|r| r.trim_start_matches('/')) {
        match ctx
            .home
            .filter(|_| is_literal_path(rest) && home == HomeState::Trusted)
        {
            Some(home_dir) => home_dir.join(rest),
            None => {
                return RoutedTarget::Unresolved {
                    reason: DegradationReason::DynamicSource,
                };
            }
        }
    } else if is_literal_path(word) {
        PathBuf::from(word)
    } else {
        return RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        };
    };
    RoutedTarget::LauncherOperand { path }
}

fn command_value_leading_word(value: &str) -> &str {
    if value.contains(char::is_whitespace) {
        skip_command_string_prefix_words(value)
            .next()
            .unwrap_or(value)
    } else {
        value
    }
}

/// Route an executor option/env value to `Unresolved` (interpreter) or `LauncherOperand` (path-like) (#384, #430).
pub(super) fn route_executor_value(
    value: &str,
    home: HomeState,
    ctx: &RouteContext<'_>,
) -> Option<RoutedTarget> {
    let stripped = value
        .strip_prefix('|')
        .or_else(|| value.strip_prefix('!'))
        .unwrap_or(value);
    if token_names_an_interpreter(stripped, ctx.trusted_aliases) {
        return Some(RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        });
    }
    let candidate = command_value_leading_word(stripped);
    if !candidate.contains('/') || candidate.contains("://") {
        return None;
    }
    Some(route_path_like_candidate(candidate, home, ctx))
}

/// Prefix words that can precede the executed command inside a quoted command string (#384, #430).
const COMMAND_STRING_PREFIX_WORDS: &[&str] =
    &["exec", "command", "builtin", "nohup", "time", "nice", "env"];

fn skip_command_string_prefix_words(tok: &str) -> std::str::SplitWhitespace<'_> {
    let mut words = tok.split_whitespace();
    loop {
        let mut lookahead = words.clone();
        let Some(word) = lookahead.next() else {
            break;
        };
        if COMMAND_STRING_PREFIX_WORDS.contains(&word)
            || word
                .split_once('=')
                .is_some_and(|(name, _)| is_shell_identifier(name))
        {
            words = lookahead;
        } else {
            break;
        }
    }
    words
}

/// Checks whole token, quoted multi-word command string's opening word, and `d4_value` right-hand side (#384, #430).
pub(super) fn token_names_an_interpreter(tok: &str, trusted_aliases: &[(&str, &str)]) -> bool {
    text_names_an_interpreter(tok, trusted_aliases)
        || d4_value(tok).is_some_and(|value| text_names_an_interpreter(value, trusted_aliases))
}

/// Unwrap nested `prefix=value` pairs (e.g. `--checkpoint-action=exec=./pyx`) to the innermost value (#384, #430, GHSA-xj54).
fn d4_value(tok: &str) -> Option<&str> {
    let (name, mut value) = tok.split_once('=')?;
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    while let Some((inner_name, inner_value)) = value.split_once('=') {
        if inner_name.is_empty() || inner_name.contains(char::is_whitespace) {
            break;
        }
        value = inner_value;
    }
    Some(value)
}

fn text_names_an_interpreter(text: &str, trusted_aliases: &[(&str, &str)]) -> bool {
    resolve_interpreter(basename(text), trusted_aliases).is_some()
        || (text.contains(char::is_whitespace)
            && skip_command_string_prefix_words(text)
                .next()
                .is_some_and(|word| resolve_interpreter(basename(word), trusted_aliases).is_some()))
}
