//! Reading one bash word the way bash runs it: quotes removed, backslash
//! escapes and ANSI-C quoting (`$'...'`) decoded.
//!
//! This crate is a leaf (ADR-022 §8) and cannot call the `aegis-parser`
//! tokenizer, so this module carries its own copy of the tokenizer's word
//! rules. `tests/bash_word_parity.rs` in the `aegis` crate runs both readers
//! over a shared table and an exhaustive set of short inputs, so the copies
//! cannot drift apart.

use std::iter::Peekable;
use std::str::Chars;

/// The word bash runs for `text` (`r\m`, `"rm"` and `$'\162m'` all read as
/// `rm`), or `None` when `text` does not read as exactly one word.
///
/// Matches the `aegis-parser` tokenizer: an ANSI-C piece that never closes
/// keeps its raw spelling, a NUL ends the decoded piece, and an unquoted
/// `$IFS` / `${IFS}` separates words.
pub fn shell_word(text: &str) -> Option<String> {
    let mut words = Words::default();
    let mut chars = text.chars().peekable();
    let mut single = false;
    let mut double = false;

    while let Some(ch) = chars.next() {
        let quoted = single || double;
        match ch {
            '\'' if !double => single = !single,
            '"' if !single => double = !double,
            '\\' if !single => match chars.next() {
                // A backslash-newline is a line continuation: both go.
                Some('\n') | None => {}
                // Inside double quotes the backslash stays unless it escapes
                // one of the characters bash gives it meaning before.
                Some(next) if double && !matches!(next, '$' | '`' | '"' | '\\') => {
                    words.current.push('\\');
                    words.current.push(next);
                }
                Some(next) => words.current.push(next),
            },
            ';' if !quoted => words.separator(";"),
            '&' if !quoted && chars.peek() == Some(&'&') => {
                chars.next();
                words.separator("&&");
            }
            // `>|` / `<|` is a redirect, not a pipe.
            '|' if !quoted && ends_with_redirect(&words.current) => words.current.push(ch),
            '|' if !quoted => {
                if chars.peek() == Some(&'|') {
                    chars.next();
                    words.separator("||");
                } else {
                    words.separator("|");
                }
            }
            c if c.is_whitespace() && !quoted => words.split(),
            '$' if !quoted && chars.peek() == Some(&'\'') => {
                chars.next();
                words.current.push_str(&read_ansi_c_quoted(&mut chars));
            }
            // `$"..."` is locale translation; without a catalog bash reads it
            // as `"..."`, so the `$` is dropped.
            '$' if !quoted && chars.peek() == Some(&'"') => {}
            '$' if !quoted => match ifs_marker_len(&chars) {
                Some(len) => {
                    words.split();
                    for _ in 1..len {
                        chars.next();
                    }
                }
                None => words.current.push(ch),
            },
            c => words.current.push(c),
        }
        if words.count > 1 {
            return None;
        }
    }
    words.split();
    match words.count {
        1 => words.first,
        _ => None,
    }
}

/// Words read so far, separators (`;`, `&&`, `|`, `||`) counted as words as
/// the tokenizer does: only the first is kept, since the caller wants exactly
/// one.
#[derive(Default)]
struct Words {
    current: String,
    first: Option<String>,
    count: usize,
}

impl Words {
    /// End the word being read, if any.
    fn split(&mut self) {
        if !self.current.is_empty() {
            let word = std::mem::take(&mut self.current);
            self.push(word);
        }
    }

    /// End the word being read, then record `sep` as a word of its own.
    fn separator(&mut self, sep: &str) {
        self.split();
        self.push(sep.to_string());
    }

    fn push(&mut self, word: String) {
        self.count += 1;
        if self.first.is_none() {
            self.first = Some(word);
        }
    }
}

/// Whether `current` ends in an unescaped `>` or `<`, so a following `|`
/// belongs to a redirect.
fn ends_with_redirect(current: &str) -> bool {
    let mut rev = current.trim_end().chars().rev();
    match rev.next() {
        Some('>' | '<') => rev.take_while(|&c| c == '\\').count() % 2 == 0,
        _ => false,
    }
}

/// The length of a literal `$IFS` (4) or `${IFS}` (6) starting at the `$`
/// just consumed; `chars` is not advanced.
fn ifs_marker_len(chars: &Peekable<Chars<'_>>) -> Option<usize> {
    let mut ahead = chars.clone();
    match ahead.next()? {
        '{' => ahead.by_ref().take(4).eq("IFS}".chars()).then_some(6),
        'I' => {
            if !ahead.by_ref().take(2).eq("FS".chars()) {
                return None;
            }
            let extends = ahead
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
            (!extends).then_some(4)
        }
        _ => None,
    }
}

/// Decode one ANSI-C quoted piece; `chars` sits just past the opening `$'`.
/// Returns the raw spelling (`$'` included) when the piece never closes.
/// Follows bash's `ansicstr`: a NUL ends the decoded text, and bytes that are
/// not valid UTF-8 are replaced.
fn read_ansi_c_quoted(chars: &mut Peekable<Chars<'_>>) -> String {
    let mut raw = String::from("$'");
    let mut bytes = Vec::new();
    let mut truncated = false;
    loop {
        let Some(ch) = chars.next() else {
            return raw;
        };
        raw.push(ch);
        if ch == '\'' {
            break;
        }
        let decoded = if ch == '\\' {
            read_escape(chars, &mut raw)
        } else {
            let mut buf = [0u8; 4];
            ch.encode_utf8(&mut buf).as_bytes().to_vec()
        };
        if truncated {
            continue;
        }
        bytes.extend_from_slice(&decoded);
        if bytes.last() == Some(&0) {
            bytes.pop();
            truncated = true;
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Decode the escape after a backslash into bytes, recording consumed
/// characters in `raw`.
fn read_escape(chars: &mut Peekable<Chars<'_>>, raw: &mut String) -> Vec<u8> {
    let Some(esc) = chars.next() else {
        return b"\\".to_vec();
    };
    raw.push(esc);
    let simple = match esc {
        'a' => Some(0x07),
        'b' => Some(0x08),
        'e' | 'E' => Some(0x1b),
        'f' => Some(0x0c),
        'n' => Some(b'\n'),
        'r' => Some(b'\r'),
        't' => Some(b'\t'),
        'v' => Some(0x0b),
        '\\' | '\'' | '"' | '?' => Some(esc as u8),
        _ => None,
    };
    if let Some(byte) = simple {
        return vec![byte];
    }
    match esc {
        '0'..='7' => {
            let mut value = esc.to_digit(8).unwrap_or(0);
            for _ in 0..2 {
                let Some(digit) = chars.peek().and_then(|c| c.to_digit(8)) else {
                    break;
                };
                value = value * 8 + digit;
                take_into(chars, raw);
            }
            // `\777` keeps only the low byte, as bash does.
            vec![(value & 0xff) as u8]
        }
        'x' => match read_hex(chars, raw, 2) {
            Some(value) => vec![(value & 0xff) as u8],
            None => b"\\x".to_vec(),
        },
        'u' | 'U' => {
            let max = if esc == 'u' { 4 } else { 8 };
            match read_hex(chars, raw, max) {
                Some(value) => match char::from_u32(value) {
                    Some(c) => c.to_string().into_bytes(),
                    None => format!("\\{esc}{value:x}").into_bytes(),
                },
                None => format!("\\{esc}").into_bytes(),
            }
        }
        'c' => read_control(chars, raw),
        other => format!("\\{other}").into_bytes(),
    }
}

/// Move the next character of `chars` into `raw`.
fn take_into(chars: &mut Peekable<Chars<'_>>, raw: &mut String) {
    if let Some(c) = chars.next() {
        raw.push(c);
    }
}

/// Read up to `max` hex digits; `None` when none follow.
fn read_hex(chars: &mut Peekable<Chars<'_>>, raw: &mut String, max: usize) -> Option<u32> {
    let mut value: Option<u32> = None;
    for _ in 0..max {
        let Some(digit) = chars.peek().and_then(|c| c.to_digit(16)) else {
            break;
        };
        value = Some(value.unwrap_or(0) * 16 + digit);
        take_into(chars, raw);
    }
    value
}

/// `\cX`: the control character for `X` (`\c?` is DEL). After `\c\` bash
/// drops a second backslash and keeps any other character.
fn read_control(chars: &mut Peekable<Chars<'_>>, raw: &mut String) -> Vec<u8> {
    match chars.peek().copied() {
        Some(c) if c.is_ascii() && c != '\'' => {
            take_into(chars, raw);
            let value = if c == '?' {
                0x7f
            } else {
                (c.to_ascii_uppercase() as u8) & 0x1f
            };
            let mut bytes = vec![value];
            if c == '\\' {
                match chars.next() {
                    Some('\\') => raw.push('\\'),
                    Some(paired) => {
                        raw.push(paired);
                        let mut buf = [0u8; 4];
                        bytes.extend_from_slice(paired.encode_utf8(&mut buf).as_bytes());
                    }
                    None => {}
                }
            }
            bytes
        }
        _ => b"\\c".to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::shell_word;

    #[test]
    fn quoted_and_escaped_spellings_read_as_the_word_bash_runs() {
        for text in [r"r\m", r#""rm""#, "r''m", r"$'\162m'", r"$'\x72m'", "r$'m'"] {
            assert_eq!(shell_word(text).as_deref(), Some("rm"), "{text}");
        }
    }

    #[test]
    fn double_quotes_keep_a_backslash_before_an_ordinary_character() {
        assert_eq!(shell_word(r#""r\m""#).as_deref(), Some(r"r\m"));
    }

    #[test]
    fn nul_ends_the_ansi_c_piece() {
        assert_eq!(shell_word(r"$'rm\0x'").as_deref(), Some("rm"));
    }

    #[test]
    fn unclosed_ansi_c_piece_keeps_its_raw_spelling() {
        assert_eq!(shell_word(r"$'\162m").as_deref(), Some(r"$'\162m"));
    }

    #[test]
    fn text_that_is_not_exactly_one_word_reads_as_none() {
        for text in ["", "''", "a b", "a;b", "a|b", "a${IFS}b"] {
            assert_eq!(shell_word(text), None, "{text:?}");
        }
    }
}
