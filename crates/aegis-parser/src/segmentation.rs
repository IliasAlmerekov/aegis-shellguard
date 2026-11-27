use super::{PipelineChain, PipelineSegment, extract_nested_commands, split_tokens};
use crate::list_segments::{
    ListSegment, finalize_segment, split_top_level_command_groups_with_separators,
};
use crate::redirection::ends_with_redirect_target;
use crate::tokenizer::suspend_ranges::{SuspendCursor, case_statement_suspend_ranges};

pub fn logical_segments(cmd: &str) -> Vec<String> {
    let mut segments = Vec::new();

    for segment in logical_scan_segments(cmd) {
        push_unique(&mut segments, segment.normalized);
    }

    segments
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanSegment {
    pub normalized: String,
    pub tokens: Vec<String>,
}

pub fn logical_scan_segments(cmd: &str) -> Vec<ScanSegment> {
    let mut segments = Vec::new();

    for raw_segment in split_top_level_segments(cmd) {
        collect_scan_segments(&raw_segment, &mut segments);
    }

    segments
}

pub fn has_multiple_top_level_segments(cmd: &str) -> bool {
    split_top_level_segments(cmd).len() > 1
}

pub fn top_level_pipelines(cmd: &str) -> Vec<PipelineChain> {
    split_top_level_command_groups(cmd)
        .into_iter()
        .filter_map(|raw_group| {
            let segments = split_pipeline_segments(&raw_group);
            (segments.len() > 1).then_some(PipelineChain {
                raw: raw_group,
                segments,
            })
        })
        .collect()
}

fn collect_scan_segments(raw_segment: &str, segments: &mut Vec<ScanSegment>) {
    let tokens = split_tokens(raw_segment);
    if !tokens.is_empty() {
        let segment = ScanSegment {
            normalized: tokens.join(" "),
            tokens,
        };
        if !segments.contains(&segment) {
            segments.push(segment);
        }
    }

    if let Some(stripped_env_command) = strip_env_prefix(raw_segment) {
        collect_scan_segments(&stripped_env_command, segments);
    }

    if let Some(stripped_syntax) = strip_leading_shell_syntax(raw_segment) {
        collect_scan_segments(&stripped_syntax, segments);
    }

    for nested in extract_nested_commands(raw_segment) {
        collect_scan_segments(&nested, segments);
    }

    if let Some(subshell_body) = unwrap_subshell_group(raw_segment) {
        for nested_segment in split_top_level_segments(&subshell_body) {
            collect_scan_segments(&nested_segment, segments);
        }
    }

    for body in extract_command_substitution_bodies(raw_segment) {
        for nested_segment in split_top_level_segments(&body) {
            collect_scan_segments(&nested_segment, segments);
        }
    }
}

pub(super) fn split_top_level_segments(cmd: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut chars = cmd.chars().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_backticks = false;
    let mut paren_depth = 0usize;
    let mut command_subst_depth = 0usize;

    while let Some(ch) = chars.next() {
        match ch {
            '\\' if !in_single_quote => {
                current.push(ch);
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            '\'' if !in_double_quote && !in_backticks => {
                in_single_quote = !in_single_quote;
                current.push(ch);
            }
            '"' if !in_single_quote && !in_backticks => {
                in_double_quote = !in_double_quote;
                current.push(ch);
            }
            '`' if !in_single_quote => {
                in_backticks = !in_backticks;
                current.push(ch);
            }
            '$' if !in_single_quote && !in_backticks && chars.peek() == Some(&'(') => {
                command_subst_depth += 1;
                current.push(ch);
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            '(' if !in_single_quote
                && !in_double_quote
                && !in_backticks
                && command_subst_depth == 0 =>
            {
                paren_depth += 1;
                current.push(ch);
            }
            ')' if !in_single_quote
                && !in_backticks
                && (command_subst_depth > 0 || paren_depth > 0) =>
            {
                if command_subst_depth > 0 {
                    command_subst_depth -= 1;
                } else {
                    paren_depth -= 1;
                }
                current.push(ch);
            }
            '\n' if !in_single_quote
                && !in_double_quote
                && !in_backticks
                && paren_depth == 0
                && command_subst_depth == 0 =>
            {
                finalize_segment(&mut current, &mut segments);
            }
            ';' if !in_single_quote
                && !in_double_quote
                && !in_backticks
                && paren_depth == 0
                && command_subst_depth == 0 =>
            {
                finalize_segment(&mut current, &mut segments);
            }
            '&' if !in_single_quote
                && !in_double_quote
                && !in_backticks
                && paren_depth == 0
                && command_subst_depth == 0 =>
            {
                if chars.peek() == Some(&'&') {
                    chars.next();
                    finalize_segment(&mut current, &mut segments);
                } else if chars.peek() != Some(&'>') && !ends_with_redirect_target(&current) {
                    finalize_segment(&mut current, &mut segments);
                } else {
                    current.push(ch);
                }
            }
            '|' if !in_single_quote
                && !in_double_quote
                && !in_backticks
                && paren_depth == 0
                && command_subst_depth == 0 =>
            {
                if chars.peek() == Some(&'|') {
                    chars.next();
                }
                finalize_segment(&mut current, &mut segments);
            }
            _ => current.push(ch),
        }
    }

    finalize_segment(&mut current, &mut segments);
    segments
}

fn split_top_level_command_groups(cmd: &str) -> Vec<String> {
    split_top_level_command_groups_with_separators(cmd)
        .into_iter()
        .map(|(raw, _separator)| raw)
        .collect()
}

pub fn list_segments(cmd: &str) -> Vec<ListSegment> {
    split_top_level_command_groups_with_separators(cmd)
        .into_iter()
        .map(|(raw_group, separator)| ListSegment {
            pipeline: PipelineChain {
                segments: split_pipeline_segments(&raw_group),
                raw: raw_group,
            },
            separator,
        })
        .collect()
}

fn split_pipeline_segments(raw_group: &str) -> Vec<PipelineSegment> {
    let mut raw_segments = Vec::new();
    let mut current = String::new();
    let case_ranges = case_statement_suspend_ranges(raw_group);
    let mut suspend = SuspendCursor::new(&case_ranges);
    let mut chars = raw_group.char_indices().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_backticks = false;
    let mut paren_depth = 0usize;
    let mut command_subst_depth = 0usize;

    while let Some((byte_idx, ch)) = chars.next() {
        if suspend.contains(byte_idx) {
            current.push(ch);
            continue;
        }
        match ch {
            '\\' if !in_single_quote => {
                current.push(ch);
                if let Some((_, next)) = chars.next() {
                    current.push(next);
                }
            }
            '\'' if !in_double_quote && !in_backticks => {
                in_single_quote = !in_single_quote;
                current.push(ch);
            }
            '"' if !in_single_quote && !in_backticks => {
                in_double_quote = !in_double_quote;
                current.push(ch);
            }
            '`' if !in_single_quote => {
                in_backticks = !in_backticks;
                current.push(ch);
            }
            '$' if !in_single_quote
                && !in_backticks
                && chars.peek().map(|&(_, c)| c) == Some('(') =>
            {
                command_subst_depth += 1;
                current.push(ch);
                if let Some((_, next)) = chars.next() {
                    current.push(next);
                }
            }
            '(' if !in_single_quote
                && !in_double_quote
                && !in_backticks
                && command_subst_depth == 0 =>
            {
                paren_depth += 1;
                current.push(ch);
            }
            ')' if !in_single_quote
                && !in_backticks
                && (command_subst_depth > 0 || paren_depth > 0) =>
            {
                if command_subst_depth > 0 {
                    command_subst_depth -= 1;
                } else {
                    paren_depth -= 1;
                }
                current.push(ch);
            }
            '|' if !in_single_quote
                && !in_double_quote
                && !in_backticks
                && paren_depth == 0
                && command_subst_depth == 0
                && chars.peek().map(|&(_, c)| c) != Some('|')
                && !ends_with_redirect_target(&current) =>
            {
                finalize_segment(&mut current, &mut raw_segments);
            }
            _ => current.push(ch),
        }
    }

    finalize_segment(&mut current, &mut raw_segments);

    raw_segments
        .into_iter()
        .filter_map(|raw| {
            normalize_segment(&raw).map(|normalized| PipelineSegment { raw, normalized })
        })
        .collect()
}

fn normalize_segment(raw_segment: &str) -> Option<String> {
    let tokens = split_tokens(raw_segment);
    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" "))
    }
}

fn push_unique(segments: &mut Vec<String>, segment: String) {
    if !segment.is_empty() && !segments.iter().any(|existing| existing == &segment) {
        segments.push(segment);
    }
}

fn strip_env_prefix(raw_segment: &str) -> Option<String> {
    let tokens = split_tokens(raw_segment);
    if tokens.is_empty() {
        return None;
    }

    let mut idx = 0;
    let mut stripped_any = false;

    if tokens.get(idx).map(String::as_str) == Some("env") {
        idx += 1;
        stripped_any = true;
    }

    while let Some(token) = tokens.get(idx) {
        if token.contains('=') && !token.starts_with('-') {
            idx += 1;
            stripped_any = true;
        } else {
            break;
        }
    }

    if stripped_any && idx < tokens.len() {
        Some(tokens[idx..].join(" "))
    } else {
        None
    }
}

fn strip_leading_shell_syntax(raw_segment: &str) -> Option<String> {
    let segment = raw_segment.trim_start();

    if strip_shell_keyword(segment, "case").is_some() {
        let tokens = split_tokens(segment);
        if tokens.get(2).is_some_and(|token| token == "in") && tokens.len() > 3 {
            return Some(tokens[3..].join(" "));
        }
    }

    if let Some(header) = strip_shell_keyword(segment, "function")
        && let Some((name, body)) = header.split_once('{')
        && !name.trim().is_empty()
        && !name.trim().chars().any(char::is_whitespace)
    {
        return Some(body.trim_start().to_string());
    }

    for prefix in std::iter::once("{").chain(crate::COMMAND_STARTING_KEYWORDS) {
        if let Some(rest) = strip_shell_keyword(segment, prefix) {
            return Some(rest.to_string());
        }
    }

    if let Some((pattern, rest)) = segment.split_once(')')
        && !pattern.is_empty()
        && !pattern.chars().any(char::is_whitespace)
        && rest.chars().next().is_some_and(char::is_whitespace)
    {
        return Some(rest.trim_start().to_string());
    }
    None
}

fn strip_shell_keyword<'a>(segment: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = segment.strip_prefix(keyword)?;
    rest.chars()
        .next()
        .is_some_and(char::is_whitespace)
        .then(|| rest.trim_start())
}

pub fn unwrap_subshell_group(raw_segment: &str) -> Option<String> {
    let trimmed = raw_segment.trim();
    if !trimmed.starts_with('(') {
        return None;
    }

    let case_ranges = case_statement_suspend_ranges(trimmed);
    let mut suspend = SuspendCursor::new(&case_ranges);
    let mut chars = trimmed.char_indices().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_backticks = false;
    let mut paren_depth = 0usize;
    let mut command_subst_depth = 0usize;
    let mut close_idx = None;

    while let Some((idx, ch)) = chars.next() {
        if suspend.contains(idx) {
            continue;
        }
        match ch {
            '\\' if !in_single_quote => {
                chars.next();
            }
            '\'' if !in_double_quote && !in_backticks => {
                in_single_quote = !in_single_quote;
            }
            '"' if !in_single_quote && !in_backticks => {
                in_double_quote = !in_double_quote;
            }
            '`' if !in_single_quote => {
                in_backticks = !in_backticks;
            }
            '$' if !in_single_quote
                && !in_backticks
                && chars.peek().map(|&(_, c)| c) == Some('(') =>
            {
                command_subst_depth += 1;
                chars.next();
            }
            '(' if !in_single_quote
                && !in_double_quote
                && !in_backticks
                && command_subst_depth == 0 =>
            {
                paren_depth += 1;
            }
            ')' if !in_single_quote
                && !in_backticks
                && (command_subst_depth > 0 || (!in_double_quote && paren_depth > 0)) =>
            {
                if command_subst_depth > 0 {
                    command_subst_depth -= 1;
                } else {
                    paren_depth -= 1;
                    if paren_depth == 0 {
                        close_idx = Some(idx);
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    if let Some(close_idx) = close_idx {
        let inner = trimmed[1..close_idx].trim();
        if !inner.is_empty() {
            return Some(inner.to_string());
        }
    }

    None
}

pub fn extract_command_substitution_bodies(raw_segment: &str) -> Vec<String> {
    let case_ranges = case_statement_suspend_ranges(raw_segment);
    let mut bodies = Vec::new();
    let mut idx = 0usize;
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    while idx < raw_segment.len() {
        let Some(ch) = raw_segment[idx..].chars().next() else {
            break;
        };
        let ch_len = ch.len_utf8();

        match ch {
            '\\' if !in_single_quote => {
                idx += ch_len;
                if let Some(next) = raw_segment[idx..].chars().next() {
                    idx += next.len_utf8();
                }
                continue;
            }
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            '$' if !in_single_quote && raw_segment[idx + ch_len..].starts_with('(') => {
                if let Some((body, end_idx)) =
                    extract_dollar_paren_body(raw_segment, idx, &case_ranges)
                {
                    bodies.push(body);
                    idx = end_idx;
                    continue;
                }
            }
            '`' if !in_single_quote => {
                if let Some((body, end_idx)) = extract_backtick_body(raw_segment, idx) {
                    bodies.push(body);
                    idx = end_idx;
                    continue;
                }
            }
            _ => {}
        }
        idx += ch_len;
    }

    bodies
}

pub fn mask_command_substitutions(text: &str) -> Option<std::borrow::Cow<'_, str>> {
    if !text.contains(['$', '`']) {
        return Some(std::borrow::Cow::Borrowed(text));
    }

    let case_ranges = case_statement_suspend_ranges(text);
    let mut kept = String::with_capacity(text.len());
    let mut idx = 0usize;
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    while let Some(ch) = text[idx..].chars().next() {
        let ch_len = ch.len_utf8();

        match ch {
            '\\' if !in_single_quote => {
                kept.push(ch);
                idx += ch_len;
                if let Some(next) = text[idx..].chars().next() {
                    kept.push(next);
                    idx += next.len_utf8();
                }
                continue;
            }
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            '$' if !in_single_quote
                && !in_double_quote
                && text[idx + ch_len..].starts_with('\'') =>
            {
                return None;
            }
            '$' if !in_single_quote && text[idx + ch_len..].starts_with('(') => {
                let (_, end_idx) = extract_dollar_paren_body(text, idx, &case_ranges)?;
                kept.push_str("$()");
                idx = end_idx;
                continue;
            }
            '`' if !in_single_quote => {
                let (_, end_idx) = extract_backtick_body(text, idx)?;
                kept.push_str("$()");
                idx = end_idx;
                continue;
            }
            _ => {}
        }
        kept.push(ch);
        idx += ch_len;
    }

    Some(std::borrow::Cow::Owned(kept))
}

fn extract_dollar_paren_body(
    raw_segment: &str,
    start_idx: usize,
    case_ranges: &[std::ops::Range<usize>],
) -> Option<(String, usize)> {
    let mut suspend = SuspendCursor::new(case_ranges);
    let body_start = start_idx + 2;
    let mut idx = body_start;
    let mut depth = 1usize;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_backticks = false;

    while idx < raw_segment.len() {
        let ch = raw_segment[idx..].chars().next()?;
        let ch_len = ch.len_utf8();

        if suspend.contains(idx) {
            idx += ch_len;
            continue;
        }

        match ch {
            '\\' if !in_single_quote => {
                idx += ch_len;
                if let Some(next) = raw_segment[idx..].chars().next() {
                    idx += next.len_utf8();
                }
                continue;
            }
            '\'' if !in_double_quote && !in_backticks => in_single_quote = !in_single_quote,
            '"' if !in_single_quote && !in_backticks => in_double_quote = !in_double_quote,
            '`' if !in_single_quote => in_backticks = !in_backticks,
            '$' if !in_single_quote
                && !in_backticks
                && raw_segment[idx + ch_len..].starts_with('(') =>
            {
                depth += 1;
                idx += ch_len;
                if let Some(next) = raw_segment[idx..].chars().next() {
                    idx += next.len_utf8();
                }
                continue;
            }
            '(' if !in_single_quote && !in_double_quote && !in_backticks => depth += 1,
            ')' if !in_single_quote && !in_backticks && !in_double_quote => {
                depth -= 1;
                if depth == 0 {
                    return Some((
                        raw_segment[body_start..idx].trim().to_string(),
                        idx + ch_len,
                    ));
                }
            }
            _ => {}
        }
        idx += ch_len;
    }

    None
}

fn extract_backtick_body(raw_segment: &str, start_idx: usize) -> Option<(String, usize)> {
    let body_start = start_idx + 1;
    let mut idx = body_start;

    while idx < raw_segment.len() {
        let ch = raw_segment[idx..].chars().next()?;
        let ch_len = ch.len_utf8();

        match ch {
            '\\' => {
                idx += ch_len;
                if let Some(next) = raw_segment[idx..].chars().next() {
                    idx += next.len_utf8();
                }
                continue;
            }
            '`' => {
                return Some((
                    raw_segment[body_start..idx].trim().to_string(),
                    idx + ch_len,
                ));
            }
            _ => {}
        }
        idx += ch_len;
    }

    None
}
