use std::ops::Range;

use super::{segmentation::split_top_level_segments, split_tokens};
use crate::heredoc_data_consumer::heredoc_target_is_data_consumer;

use aegis_types::InlineScript;

#[derive(Debug, PartialEq)]
pub struct HeredocBody {
    pub delimiter: String,
    pub body: String,
    pub is_nowdoc: bool,
    pub target_is_interpreter: bool,
    pub is_data_consumer_target: bool,
}

const STDIN_EXECUTING_PROGRAMS: &[&str] = &[
    "bash", "sh", "zsh", "dash", "ash", "ksh", "python", "python3", "node", "nodejs", "ruby",
    "php", "lua", "perl",
];

fn heredoc_target_program(line: &str, marker_start: usize) -> Option<&str> {
    let prefix = line[..marker_start].trim_end();
    let end = prefix.len();
    let mut start = end;

    for (idx, ch) in prefix.char_indices().rev() {
        if ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '.' || ch == '/' {
            start = idx;
        } else {
            break;
        }
    }

    if start == end {
        return None;
    }

    let token = &prefix[start..end];
    let basename = token.rsplit_once('/').map_or(token, |(_, tail)| tail);
    if basename.is_empty() {
        None
    } else {
        Some(basename)
    }
}

pub fn extract_process_substitution_bodies(cmd: &str) -> Vec<String> {
    let chars: Vec<char> = cmd.chars().collect();
    let mut bodies = Vec::new();
    let mut i = 0;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_backticks = false;

    while i < chars.len() {
        match chars[i] {
            '\\' if !in_single_quote => {
                i = (i + 2).min(chars.len());
            }
            '\'' if !in_double_quote && !in_backticks => {
                in_single_quote = !in_single_quote;
                i += 1;
            }
            '"' if !in_single_quote && !in_backticks => {
                in_double_quote = !in_double_quote;
                i += 1;
            }
            '`' if !in_single_quote => {
                in_backticks = !in_backticks;
                i += 1;
            }
            '<' | '>'
                if !in_single_quote
                    && !in_double_quote
                    && !in_backticks
                    && chars.get(i + 1) == Some(&'(') =>
            {
                if let Some((body, end_idx)) = extract_angle_paren_body(&chars, i) {
                    bodies.push(body);
                    i = end_idx + 1;
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }

    bodies
}

fn extract_angle_paren_body(chars: &[char], start_idx: usize) -> Option<(String, usize)> {
    let mut body = String::new();
    let mut idx = start_idx + 2;
    let mut depth = 1usize;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_backticks = false;

    while idx < chars.len() {
        match chars[idx] {
            '\\' if !in_single_quote => {
                body.push(chars[idx]);
                idx += 1;
                if let Some(next) = chars.get(idx) {
                    body.push(*next);
                    idx += 1;
                }
            }
            '\'' if !in_double_quote && !in_backticks => {
                in_single_quote = !in_single_quote;
                body.push(chars[idx]);
                idx += 1;
            }
            '"' if !in_single_quote && !in_backticks => {
                in_double_quote = !in_double_quote;
                body.push(chars[idx]);
                idx += 1;
            }
            '`' if !in_single_quote => {
                in_backticks = !in_backticks;
                body.push(chars[idx]);
                idx += 1;
            }
            '(' if !in_single_quote && !in_double_quote && !in_backticks => {
                depth += 1;
                body.push(chars[idx]);
                idx += 1;
            }
            ')' if !in_single_quote && !in_double_quote && !in_backticks => {
                depth -= 1;
                if depth == 0 {
                    return Some((body.trim().to_string(), idx));
                }
                body.push(chars[idx]);
                idx += 1;
            }
            _ => {
                body.push(chars[idx]);
                idx += 1;
            }
        }
    }

    None
}

pub fn extract_eval_payloads(cmd: &str) -> Vec<String> {
    let mut payloads = Vec::new();

    for segment in split_top_level_segments(cmd) {
        let tokens = split_tokens(&segment);
        if tokens.is_empty() {
            continue;
        }

        let mut idx = 0;

        if tokens.get(idx).map(String::as_str) == Some("env") {
            idx += 1;
        }

        while let Some(token) = tokens.get(idx) {
            if token.contains('=') && !token.starts_with('-') {
                idx += 1;
            } else {
                break;
            }
        }

        if tokens.get(idx).map(String::as_str) == Some("eval") && idx + 1 < tokens.len() {
            payloads.push(tokens[idx + 1..].join(" "));
        }
    }

    payloads
}

struct HeredocMarker {
    delimiter: String,
    is_nowdoc: bool,
    strip_tabs: bool,
    operator_start: usize,
    delimiter_end: usize,
}

struct SubstitutionFrame {
    command_sub_depth: Option<usize>,
    saved_quotes: (bool, bool),
}

fn find_unquoted_double_lt(line: &str) -> Option<usize> {
    let mut single_quote = false;
    let mut double_quote = false;
    let mut frames: Vec<SubstitutionFrame> = Vec::new();
    let mut chars = line.char_indices().peekable();

    while let Some((idx, ch)) = chars.next() {
        match ch {
            '\\' if !single_quote => {
                chars.next();
            }
            '\'' if !double_quote => single_quote = !single_quote,
            '"' if !single_quote => double_quote = !double_quote,
            '`' if !single_quote => {
                if matches!(frames.last(), Some(f) if f.command_sub_depth.is_none()) {
                    if let Some(frame) = frames.pop() {
                        (single_quote, double_quote) = frame.saved_quotes;
                    }
                } else {
                    frames.push(SubstitutionFrame {
                        command_sub_depth: None,
                        saved_quotes: (single_quote, double_quote),
                    });
                    single_quote = false;
                    double_quote = false;
                }
            }
            '$' if !single_quote && chars.peek().map(|&(_, c)| c) == Some('(') => {
                chars.next();
                frames.push(SubstitutionFrame {
                    command_sub_depth: Some(1),
                    saved_quotes: (single_quote, double_quote),
                });
                single_quote = false;
                double_quote = false;
            }
            '(' if !single_quote && !double_quote => {
                if let Some(depth) = frames.last_mut().and_then(|f| f.command_sub_depth.as_mut()) {
                    *depth += 1;
                }
            }
            ')' if !single_quote && !double_quote => {
                let closed = if let Some(depth) =
                    frames.last_mut().and_then(|f| f.command_sub_depth.as_mut())
                {
                    *depth -= 1;
                    *depth == 0
                } else {
                    false
                };
                if closed && let Some(frame) = frames.pop() {
                    (single_quote, double_quote) = frame.saved_quotes;
                }
            }
            '<' if !single_quote && !double_quote && chars.peek().map(|&(_, c)| c) == Some('<') => {
                return Some(idx);
            }
            _ => {}
        }
    }

    None
}

fn find_heredoc_marker(line: &str) -> Option<HeredocMarker> {
    let operator_start = find_unquoted_double_lt(line)?;
    let after_op = &line[operator_start + 2..];
    let (strip_tabs, after_dash) = match after_op.strip_prefix('-') {
        Some(stripped) => (true, stripped),
        None => (false, after_op),
    };
    let trimmed = after_dash.trim_start();
    let spec_start =
        operator_start + 2 + (strip_tabs as usize) + (after_dash.len() - trimmed.len());

    let mk = |delimiter: String, is_nowdoc: bool, delimiter_end: usize| HeredocMarker {
        delimiter,
        is_nowdoc,
        strip_tabs,
        operator_start,
        delimiter_end,
    };

    if let Some(inner) = trimmed.strip_prefix('\'') {
        let close = inner.find('\'')?;
        let delim = &inner[..close];
        return (!delim.is_empty())
            .then(|| mk(delim.to_string(), true, spec_start + 1 + close + 1));
    }

    if let Some(inner) = trimmed.strip_prefix('"') {
        let close = inner.find('"')?;
        let delim = &inner[..close];
        return (!delim.is_empty())
            .then(|| mk(delim.to_string(), true, spec_start + 1 + close + 1));
    }

    if let Some(inner) = trimmed.strip_prefix('\\') {
        let word: String = inner
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let word_len = word.len();
        return (!word.is_empty()).then(|| mk(word, true, spec_start + 1 + word_len));
    }

    let word: String = trimmed
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    let word_len = word.len();

    (!word.is_empty()).then(|| mk(word, false, spec_start + word_len))
}

pub fn split_at_heredoc_marker(line: &str) -> Option<(&str, &str)> {
    let marker = find_heredoc_marker(line)?;
    Some((
        &line[..marker.operator_start],
        &line[marker.delimiter_end..],
    ))
}

pub(crate) fn heredoc_suspend_ranges(cmd: &str) -> Vec<Range<usize>> {
    if !cmd.contains("<<") {
        return Vec::new();
    }

    let indexed = indexed_lines(cmd);
    let lines: Vec<&str> = indexed.iter().map(|&(line, _)| line).collect();
    let mut ranges = Vec::new();

    walk_heredocs(
        &lines,
        |marker, _interpreter, _redirects, body_range, suspend_end| {
            let Some(marker_line_index) = body_range.start.checked_sub(1) else {
                return;
            };
            let Some(&(_, marker_line_start)) = indexed.get(marker_line_index) else {
                return;
            };
            let start = marker_line_start + marker.operator_start;
            let end = match indexed.get(suspend_end) {
                Some(&(_, line_start)) if suspend_end < body_range.end => line_start,
                Some(&(terminator_line, terminator_start)) => {
                    terminator_start + terminator_line.len()
                }
                None => cmd.len(),
            };
            ranges.push(start..end);
        },
    );

    ranges
}

fn indexed_lines(cmd: &str) -> Vec<(&str, usize)> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (idx, ch) in cmd.char_indices() {
        if ch == '\n' {
            let raw = &cmd[start..idx];
            lines.push((raw.strip_suffix('\r').unwrap_or(raw), start));
            start = idx + 1;
        }
    }
    if start < cmd.len() {
        let raw = &cmd[start..];
        lines.push((raw.strip_suffix('\r').unwrap_or(raw), start));
    }
    lines
}

fn walk_heredocs(
    lines: &[&str],
    mut on_heredoc: impl FnMut(&HeredocMarker, bool, bool, Range<usize>, usize),
) {
    if !lines.iter().any(|line| line.contains("<<")) {
        return;
    }

    let mut i = 0;
    let mut command_text = String::new();

    while i < lines.len() {
        if let Some(marker) = find_heredoc_marker(lines[i]) {
            let marker_line = lines[i];
            let preceding_len = command_text.len();
            command_text.push_str(marker_line);
            command_text.push('\n');
            i += 1;
            let body_start = i;

            let mut first_prefixed = None;
            let mut terminator_exact = false;
            while i < lines.len() {
                let candidate = if marker.strip_tabs {
                    lines[i].trim_start_matches('\t')
                } else {
                    lines[i]
                };
                terminator_exact = candidate == marker.delimiter;
                if terminator_exact {
                    break;
                }
                if first_prefixed.is_none() && candidate.starts_with(marker.delimiter.as_str()) {
                    first_prefixed = Some(i);
                }
                i += 1;
            }
            let body_end = i;
            let suspend_end = first_prefixed.unwrap_or(body_end);

            let target_is_interpreter = heredoc_target_program(marker_line, marker.operator_start)
                .is_some_and(|program| {
                    STDIN_EXECUTING_PROGRAMS
                        .iter()
                        .any(|known| program.eq_ignore_ascii_case(known))
                });
            let is_data_consumer_target = terminator_exact
                && first_prefixed.is_none()
                && heredoc_target_is_data_consumer(
                    &command_text[..preceding_len],
                    marker_line,
                    marker.operator_start,
                    marker.delimiter_end,
                );

            on_heredoc(
                &marker,
                target_is_interpreter,
                is_data_consumer_target,
                body_start..body_end,
                suspend_end,
            );
        } else {
            command_text.push_str(lines[i]);
            command_text.push('\n');
        }
        i += 1;
    }
}

pub fn extract_heredoc_bodies(cmd: &str) -> Vec<HeredocBody> {
    let mut bodies = Vec::new();
    let lines: Vec<&str> = cmd.lines().collect();

    walk_heredocs(
        &lines,
        |marker, target_is_interpreter, is_data_consumer_target, body_range, _suspend_end| {
            bodies.push(HeredocBody {
                delimiter: marker.delimiter.clone(),
                body: lines[body_range].join("\n"),
                is_nowdoc: marker.is_nowdoc,
                target_is_interpreter,
                is_data_consumer_target,
            });
        },
    );

    bodies
}

pub fn mask_inert_heredoc_substitution_markers(cmd: &str) -> String {
    if !cmd.contains("<<") {
        return cmd.to_string();
    }

    let lines: Vec<&str> = cmd.lines().collect();
    let mut output_lines: Vec<String> = lines.iter().map(|line| (*line).to_string()).collect();

    walk_heredocs(
        &lines,
        |marker, target_is_interpreter, is_data_consumer_target, body_range, suspend_end| {
            if marker.is_nowdoc && !target_is_interpreter {
                for idx in body_range.start..suspend_end {
                    output_lines[idx] = if is_data_consumer_target {
                        " ".repeat(lines[idx].chars().count())
                    } else {
                        mask_substitution_markers(lines[idx])
                    };
                }
            }
        },
    );

    output_lines.join("\n")
}

fn mask_substitution_markers(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '`' {
            out.push(' ');
        } else if ch == '$' && chars.peek() == Some(&'(') {
            chars.next();
            out.push_str("$ ");
        } else {
            out.push(ch);
        }
    }

    out
}

pub fn extract_inline_scripts(cmd: &str) -> Vec<InlineScript> {
    const INTERPRETERS: &[(&str, &str)] = &[
        ("python", "-c"),
        ("python3", "-c"),
        ("node", "-e"),
        ("nodejs", "-e"),
        ("ruby", "-e"),
        ("php", "-r"),
        ("lua", "-e"),
        ("perl", "-e"),
    ];

    let mut scripts = Vec::new();
    for segment in split_top_level_segments(cmd) {
        let tokens = split_tokens(&segment);
        let mut i = 0;

        while i < tokens.len() {
            for &(interp, flag) in INTERPRETERS {
                if tokens[i] == interp {
                    if let Some(rel) = tokens[i..].iter().position(|t| t == flag) {
                        let body_idx = i + rel + 1;
                        if let Some(body) = tokens.get(body_idx) {
                            scripts.push(InlineScript {
                                interpreter: interp.to_string(),
                                body: body.clone(),
                            });
                        }
                    }
                    break;
                }
            }
            i += 1;
        }
    }

    scripts
}

#[cfg(test)]
mod tests {
    use super::{find_heredoc_marker, heredoc_suspend_ranges};

    #[test]
    fn a_double_lt_inside_dollar_paren_within_double_quotes_is_a_marker() {
        let line = "\"$(cat <<'EOF'";
        assert!(find_heredoc_marker(line).is_some());
    }

    #[test]
    fn a_double_lt_inside_a_plain_double_quoted_string_is_not_a_marker() {
        let line = "\"<<EOF\"";
        assert!(find_heredoc_marker(line).is_none());
    }

    #[test]
    fn a_double_lt_inside_a_backtick_body_is_a_marker() {
        let line = "`cat <<EOF`";
        assert!(find_heredoc_marker(line).is_some());
    }

    #[test]
    fn a_double_lt_inside_nested_dollar_paren_command_substitutions_is_a_marker() {
        let line = "\"$(echo \"$(cat <<EOF)\")\"";
        assert!(find_heredoc_marker(line).is_some());
    }

    #[test]
    fn a_double_lt_inside_dollar_paren_paren_arithmetic_matches_pre_fix_looseness() {
        let line = "echo $((1 << 2))";
        assert!(find_heredoc_marker(line).is_some());
    }

    #[test]
    fn a_heredoc_with_no_body_lines_suspends_marker_through_terminator() {
        let cmd = "cat <<EOF\nEOF\n";
        assert_eq!(
            heredoc_suspend_ranges(cmd),
            vec![cmd.find("<<").unwrap()..(cmd.rfind("EOF").unwrap() + "EOF".len())]
        );
    }

    #[test]
    fn a_heredoc_marker_with_nothing_after_it_suspends_to_the_end_of_cmd() {
        let cmd = "cat <<EOF";
        assert_eq!(
            heredoc_suspend_ranges(cmd),
            vec![cmd.find("<<").unwrap()..cmd.len()]
        );
    }

    #[test]
    fn a_crlf_heredoc_suspends_marker_through_terminator() {
        let cmd = "cat <<EOF\r\nbody\r\nEOF\r\n";
        assert_eq!(
            heredoc_suspend_ranges(cmd),
            vec![cmd.find("<<").unwrap()..(cmd.rfind("EOF").unwrap() + "EOF".len())]
        );
    }

    #[test]
    fn a_heredoc_terminator_with_no_trailing_newline_still_ends_the_suspend_range() {
        let cmd = "cat <<EOF\nbody\nEOF";
        assert_eq!(
            heredoc_suspend_ranges(cmd),
            vec![cmd.find("<<").unwrap()..(cmd.rfind("EOF").unwrap() + "EOF".len())]
        );
    }
}
