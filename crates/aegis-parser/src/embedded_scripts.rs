use std::ops::Range;

use super::{segmentation::split_top_level_segments, split_tokens};
use crate::heredoc_data_consumer::heredoc_target_is_data_consumer;
use crate::tokenizer::suspend_ranges::{HeredocMarker, find_heredoc_marker, walk_heredoc_sites};

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

pub fn split_at_heredoc_marker(line: &str) -> Option<(&str, &str)> {
    let marker = find_heredoc_marker(line)?;
    Some((
        &line[..marker.operator_start],
        &line[marker.delimiter_end..],
    ))
}

fn walk_heredocs(
    lines: &[&str],
    mut on_heredoc: impl FnMut(&HeredocMarker, bool, bool, Range<usize>, usize),
) {
    walk_heredoc_sites(lines, |site| {
        let target_is_interpreter =
            heredoc_target_program(site.marker_line, site.marker.operator_start).is_some_and(
                |program| {
                    STDIN_EXECUTING_PROGRAMS
                        .iter()
                        .any(|known| program.eq_ignore_ascii_case(known))
                },
            );
        let is_data_consumer_target = site.terminator_is_clean
            && heredoc_target_is_data_consumer(
                site.preceding_text,
                site.marker_line,
                site.marker.operator_start,
                site.marker.delimiter_end,
            );

        on_heredoc(
            site.marker,
            target_is_interpreter,
            is_data_consumer_target,
            site.body_range,
            site.suspend_end,
        );
    });
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
