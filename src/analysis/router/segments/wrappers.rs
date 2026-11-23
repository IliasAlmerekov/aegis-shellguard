//! Wrapper-body peeling for `case`/`function`/`coproc` and grammar wrappers (#384, #430).

use super::*;

fn strip_group_or_subshell_wrapper(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    trimmed
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .or_else(|| trimmed.strip_prefix('(').and_then(|s| s.strip_suffix(')')))
}

/// Raw-substring `case` arm extraction (preserves inline quoting across `;;`/`;;&`/`;&`) (#384).
fn case_arm_bodies(rest: &str) -> Vec<String> {
    let Some((_word, mut remaining)) = rest.split_once(" in ") else {
        return Vec::new();
    };
    let mut bodies = Vec::new();

    loop {
        let trimmed = remaining.trim_start();
        let Some((label, after_label)) = trimmed.split_once(')') else {
            break;
        };
        if label.is_empty() || label.chars().any(char::is_whitespace) {
            break;
        }
        match find_case_arm_terminator(after_label) {
            Some((pos, len)) => {
                bodies.push(after_label[..pos].trim().to_owned());
                remaining = &after_label[pos + len..];
            }
            None => {
                bodies.push(strip_trailing_esac(after_label).trim().to_owned());
                break;
            }
        }
    }
    bodies
}

/// Quote-aware scan for `;;&`, `;;`, or `;&` (longest match first so `;;&` is not split).
fn find_case_arm_terminator(s: &str) -> Option<(usize, usize)> {
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut chars = s.char_indices().peekable();

    while let Some((idx, ch)) = chars.next() {
        match ch {
            '\\' if !in_single_quote => {
                chars.next();
            }
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
            }
            ';' if !in_single_quote && !in_double_quote => {
                if s[idx..].starts_with(";;&") {
                    return Some((idx, 3));
                }
                if s[idx..].starts_with(";;") || s[idx..].starts_with(";&") {
                    return Some((idx, 2));
                }
            }
            _ => {}
        }
    }
    None
}

fn strip_trailing_esac(s: &str) -> &str {
    let trimmed = s.trim_end();
    trimmed.strip_suffix("esac").map_or(trimmed, str::trim_end)
}

/// POSIX `NAME() BODY` body; routed conservatively at definition time since call sites are not tracked (#384).
pub(super) fn posix_function_definition_body(trimmed: &str) -> Option<&str> {
    let paren_pos = trimmed.find('(')?;
    let name = trimmed[..paren_pos].trim_end();
    if name.is_empty()
        || name
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '$' | '`' | '/' | '{' | '}' | ';' | '|'))
    {
        return None;
    }
    let after_close = trimmed[paren_pos + 1..].strip_prefix(')')?;
    let body = after_close.trim_start();
    (!body.is_empty()).then_some(body)
}

/// Bash `function NAME [()] BODY` body; routed at definition time (#384).
fn function_keyword_body(rest: &str) -> Option<&str> {
    let name_end = rest.find(|c: char| c.is_whitespace() || c == '(')?;
    let name = &rest[..name_end];
    if name.is_empty()
        || name
            .chars()
            .any(|c| matches!(c, '$' | '`' | '{' | '}' | ';' | '|'))
    {
        return None;
    }
    let tail = rest[name_end..].trim_start();
    let body = match tail.strip_prefix('(') {
        Some(after_open) if after_open.trim_start().starts_with(')') => after_open
            .trim_start()
            .strip_prefix(')')
            .map_or("", str::trim_start),
        // Non-empty `(` opens a subshell body, so keep it.
        _ => tail,
    };
    (!body.is_empty()).then_some(body)
}

/// `coproc [NAME] { BODY; }`; bare `coproc <cmd>` is already handled by launcher stripping (#384).
fn coproc_body(rest: &str) -> Option<&str> {
    if let Some(body) = strip_group_or_subshell_wrapper(rest) {
        return Some(body);
    }
    let (name, tail) = rest.split_once(char::is_whitespace)?;
    if name.is_empty() || name.chars().any(|c| matches!(c, '{' | '}' | '(' | ')')) {
        return None;
    }
    strip_group_or_subshell_wrapper(tail.trim_start())
}

/// Strip trailing redirections using raw-substring boundaries so wrapper punctuation stays clean (#384).
pub(super) fn strip_trailing_redirection(s: &str) -> &str {
    let mut rest = s.trim_end();
    loop {
        let (before, last_word) = split_at_last_whitespace(rest).unwrap_or(("", rest));
        if last_word.is_empty() {
            return rest;
        }
        if aegis_parser::starts_with_redirection_glyph(last_word) {
            rest = before.trim_end();
            continue;
        }
        let before = before.trim_end();
        let (before_before, prior_word) = split_at_last_whitespace(before).unwrap_or(("", before));
        if aegis_parser::is_redirection_operator(prior_word) {
            rest = before_before.trim_end();
            continue;
        }
        return rest;
    }
}

/// Uses `len_utf8()` because Unicode whitespace (e.g. U+0085) can be multi-byte.
fn split_at_last_whitespace(value: &str) -> Option<(&str, &str)> {
    let (index, whitespace) = value
        .char_indices()
        .rfind(|(_, character)| character.is_whitespace())?;
    let after_whitespace = index + whitespace.len_utf8();
    Some((&value[..index], &value[after_whitespace..]))
}

/// Extract raw wrapper bodies without dequoting, so inline `-c`/`-e` quoting survives the second pass (GHSA-mgwj-4828-3mrg).
pub(super) fn wrapper_bodies(stage_raw: &str, trusted_aliases: &[(&str, &str)]) -> Vec<String> {
    let mut bodies = Vec::new();
    let trimmed = strip_trailing_redirection(stage_raw.trim_start());

    for kw in aegis_parser::COMMAND_STARTING_KEYWORDS
        .iter()
        .chain(WRAPPER_ONLY_RESERVED_WORDS)
    {
        if let Some(rest) = trimmed.strip_prefix(kw)
            && rest.starts_with(char::is_whitespace)
        {
            let rest = rest.trim_start();
            match *kw {
                "case" => bodies.extend(case_arm_bodies(rest)),
                "function" => {
                    if let Some(body) = function_keyword_body(rest) {
                        bodies.push(body.to_owned());
                    }
                }
                "coproc" => match coproc_body(rest) {
                    Some(body) => bodies.push(body.to_owned()),
                    None => bodies.push(rest.to_owned()),
                },
                _ => bodies.push(rest.to_owned()),
            }
        }
    }
    if let Some(body) = posix_function_definition_body(trimmed) {
        bodies.push(body.to_owned());
    }
    if let Some(inner) = aegis_parser::unwrap_subshell_group(trimmed) {
        bodies.push(inner);
    } else if let Some(inner) = strip_group_or_subshell_wrapper(trimmed) {
        bodies.push(inner.to_owned());
    }
    bodies.extend(aegis_parser::extract_command_substitution_bodies(stage_raw));
    bodies.extend(aegis_parser::extract_process_substitution_bodies(stage_raw));
    if command_has_heredoc(stage_raw)
        && heredoc_write_then_exec_reuse(stage_raw, trusted_aliases).is_none()
        && let Some(tail) = heredoc_marker_line_tail(stage_raw)
    {
        // Skip tail when `heredoc_write_then_exec_reuse` already routed it directly.
        bodies.push(tail.to_owned());
    }
    bodies
}
