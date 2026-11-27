use std::iter::Peekable;
use std::str::Chars;

mod ansi_c;
pub(crate) mod quote_scan;
pub(crate) mod suspend_ranges;

use crate::redirection::ends_with_redirect_target;
use ansi_c::read_ansi_c_quoted;
pub use ansi_c::{canonical_ansi_c_quoting, canonical_ansi_c_quoting_marked};

pub fn split_tokens(cmd: &str) -> Vec<String> {
    split_tokens_checked(cmd).0
}

pub fn ansi_c_quoting_is_inexact(cmd: &str) -> bool {
    cmd.contains("$'") && !split_tokens_checked(cmd).1
}

fn split_tokens_checked(cmd: &str) -> (Vec<String>, bool) {
    let mut exact = true;
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = cmd.chars().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    while let Some(ch) = chars.next() {
        match ch {
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
            }
            '\\' if !in_single_quote => match chars.next() {
                Some('\n') | None => {}
                Some(next) if in_double_quote && !matches!(next, '$' | '`' | '"' | '\\') => {
                    current.push('\\');
                    current.push(next);
                }
                Some(next) => current.push(next),
            },
            ';' if !in_single_quote && !in_double_quote => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
                tokens.push(";".to_string());
            }
            '&' if !in_single_quote && !in_double_quote => {
                if chars.peek() == Some(&'&') {
                    chars.next();
                    if !current.is_empty() {
                        tokens.push(current.clone());
                        current.clear();
                    }
                    tokens.push("&&".to_string());
                } else {
                    current.push(ch);
                }
            }
            '|' if !in_single_quote && !in_double_quote && ends_with_redirect_target(&current) => {
                current.push(ch);
            }
            '|' if !in_single_quote && !in_double_quote => {
                if chars.peek() == Some(&'|') {
                    chars.next();
                    if !current.is_empty() {
                        tokens.push(current.clone());
                        current.clear();
                    }
                    tokens.push("||".to_string());
                } else {
                    if !current.is_empty() {
                        tokens.push(current.clone());
                        current.clear();
                    }
                    tokens.push("|".to_string());
                }
            }
            c if c.is_whitespace() && !in_single_quote && !in_double_quote => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            '$' if !in_single_quote && !in_double_quote && chars.peek() == Some(&'\'') => {
                chars.next();
                let piece = read_ansi_c_quoted(&mut chars);
                exact &= piece.exact;
                current.push_str(&piece.text);
            }
            '$' if !in_single_quote && !in_double_quote && chars.peek() == Some(&'"') => {}
            '$' if !in_single_quote && !in_double_quote => {
                if let Some(marker_len) = ifs_marker_len(&chars) {
                    if !current.is_empty() {
                        tokens.push(current.clone());
                        current.clear();
                    }
                    for _ in 1..marker_len {
                        chars.next();
                    }
                } else {
                    current.push(ch);
                }
            }
            c => {
                current.push(c);
            }
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    (tokens, exact)
}

fn ifs_marker_len(chars: &Peekable<Chars<'_>>) -> Option<usize> {
    let mut lookahead = chars.clone();
    match lookahead.next()? {
        '{' => {
            let braces_wrap_ifs = lookahead.next() == Some('I')
                && lookahead.next() == Some('F')
                && lookahead.next() == Some('S')
                && lookahead.next() == Some('}');
            braces_wrap_ifs.then_some(6)
        }
        'I' => {
            let spells_ifs = lookahead.next() == Some('F') && lookahead.next() == Some('S');
            if !spells_ifs {
                return None;
            }
            let extends_identifier = lookahead
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
            (!extends_identifier).then_some(4)
        }
        _ => None,
    }
}

pub fn extract_prefix(tokens: &[String]) -> Vec<String> {
    if tokens.is_empty() {
        return Vec::new();
    }

    let mut prefix = Vec::new();
    prefix.push(tokens[0].clone());

    let has_any_flag = tokens.iter().skip(1).any(|t| t.starts_with('-'));

    for (i, token) in tokens.iter().skip(1).enumerate() {
        if token == "--" {
            break;
        }
        if token.starts_with('-') {
            prefix.push(token.clone());
            continue;
        }
        if token.starts_with('/')
            || token.starts_with('.')
            || token.starts_with('~')
            || token.contains('/')
            || token.contains('.')
        {
            break;
        }
        if !has_any_flag && tokens.len() > 3 && i > 0 {
            break;
        }
        prefix.push(token.clone());
    }

    prefix
}
