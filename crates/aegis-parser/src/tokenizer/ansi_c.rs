use std::borrow::Cow;
use std::iter::Peekable;
use std::str::Chars;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AnsiCPiece {
    pub(crate) text: String,
    pub(crate) raw: String,
    pub(crate) exact: bool,
}

pub(crate) fn read_ansi_c_quoted(chars: &mut Peekable<Chars<'_>>) -> AnsiCPiece {
    let mut raw = String::from("$'");
    let mut bytes = Vec::new();
    let mut exact = true;
    let mut truncated = false;

    loop {
        let Some(ch) = chars.next() else {
            return AnsiCPiece {
                text: raw.clone(),
                raw,
                exact: false,
            };
        };
        raw.push(ch);
        if ch == '\'' {
            break;
        }
        let mut decoded = Decoded::Char(ch);
        if ch == '\\' {
            decoded = read_escape(chars, &mut raw);
        }
        if truncated {
            continue;
        }
        match decoded {
            Decoded::Char(c) => push_char(&mut bytes, c),
            Decoded::Byte(b) => bytes.push(b),
            Decoded::Bytes(more) => bytes.extend_from_slice(&more),
            Decoded::Literal(text) => bytes.extend_from_slice(text.as_bytes()),
            Decoded::Invalid(text) => {
                exact = false;
                bytes.extend_from_slice(text.as_bytes());
            }
        }
        if bytes.last() == Some(&0) {
            bytes.pop();
            truncated = true;
            exact = false;
        }
    }

    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(err) => {
            exact = false;
            String::from_utf8_lossy(err.as_bytes()).into_owned()
        }
    };
    AnsiCPiece { text, raw, exact }
}

pub fn canonical_ansi_c_quoting(cmd: &str) -> Cow<'_, str> {
    canonicalize(cmd, false)
}

pub fn canonical_ansi_c_quoting_marked(cmd: &str) -> Cow<'_, str> {
    canonicalize(cmd, true)
}

fn canonicalize(cmd: &str, keep_marker: bool) -> Cow<'_, str> {
    if !cmd.contains("$'") && !cmd.contains("$\"") {
        return Cow::Borrowed(cmd);
    }

    let mut out = String::with_capacity(cmd.len());
    let mut chars = cmd.chars().peekable();
    let mut frames = vec![QuoteFrame::new(FrameKind::Command)];

    while let Some(ch) = chars.next() {
        let Some(frame) = frames.last_mut() else {
            break;
        };
        let quoted = frame.single || frame.double;
        match ch {
            '\\' if !frame.single => {
                out.push(ch);
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            }
            '\'' if !frame.double => {
                frame.single = !frame.single;
                out.push(ch);
            }
            '"' if !frame.single => {
                frame.double = !frame.double;
                out.push(ch);
            }
            '$' if !frame.single && chars.peek() == Some(&'(') => {
                chars.next();
                out.push_str("$(");
                frames.push(QuoteFrame::new(FrameKind::Parens(1)));
            }
            '`' if !frame.single => {
                out.push(ch);
                if frame.kind == FrameKind::Backtick && !frame.double {
                    frames.pop();
                } else {
                    frames.push(QuoteFrame::new(FrameKind::Backtick));
                }
            }
            '(' if !quoted => {
                if let FrameKind::Parens(depth) = &mut frame.kind {
                    *depth += 1;
                }
                out.push(ch);
            }
            ')' if !quoted => {
                if let FrameKind::Parens(depth) = &mut frame.kind {
                    *depth -= 1;
                    if *depth == 0 {
                        frames.pop();
                    }
                }
                out.push(ch);
            }
            '$' if !quoted && chars.peek() == Some(&'\'') => {
                chars.next();
                let piece = read_ansi_c_quoted(&mut chars);
                if piece.exact {
                    if keep_marker {
                        out.push_str("$''");
                    }
                    push_single_quoted(&mut out, &piece.text);
                } else {
                    out.push_str(&piece.raw);
                }
            }
            '$' if !quoted && chars.peek() == Some(&'"') => {}
            _ => out.push(ch),
        }
    }

    Cow::Owned(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameKind {
    Command,
    Parens(usize),
    Backtick,
}

struct QuoteFrame {
    kind: FrameKind,
    single: bool,
    double: bool,
}

impl QuoteFrame {
    fn new(kind: FrameKind) -> Self {
        Self {
            kind,
            single: false,
            double: false,
        }
    }
}

fn push_single_quoted(out: &mut String, text: &str) {
    out.push('\'');
    for c in text.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
}

enum Decoded {
    Char(char),
    Byte(u8),
    Bytes(Vec<u8>),
    Literal(String),
    Invalid(String),
}

fn push_char(bytes: &mut Vec<u8>, c: char) {
    let mut buf = [0u8; 4];
    bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
}

fn read_escape(chars: &mut Peekable<Chars<'_>>, raw: &mut String) -> Decoded {
    let Some(esc) = chars.next() else {
        return Decoded::Literal("\\".to_string());
    };
    raw.push(esc);
    let simple = match esc {
        'a' => Some('\x07'),
        'b' => Some('\x08'),
        'e' | 'E' => Some('\x1b'),
        'f' => Some('\x0c'),
        'n' => Some('\n'),
        'r' => Some('\r'),
        't' => Some('\t'),
        'v' => Some('\x0b'),
        '\\' | '\'' | '"' | '?' => Some(esc),
        _ => None,
    };
    if let Some(c) = simple {
        return Decoded::Char(c);
    }
    match esc {
        '0'..='7' => {
            let mut value = esc.to_digit(8).unwrap_or(0);
            for _ in 0..2 {
                match chars.peek().and_then(|c| c.to_digit(8)) {
                    Some(digit) => {
                        value = value * 8 + digit;
                        if let Some(c) = chars.next() {
                            raw.push(c);
                        }
                    }
                    None => break,
                }
            }
            Decoded::Byte((value & 0xff) as u8)
        }
        'x' => match read_hex(chars, raw, 2) {
            Some(value) => Decoded::Byte((value & 0xff) as u8),
            None => Decoded::Literal("\\x".to_string()),
        },
        'u' | 'U' => {
            let max = if esc == 'u' { 4 } else { 8 };
            match read_hex(chars, raw, max) {
                Some(value) => match char::from_u32(value) {
                    Some(c) => Decoded::Char(c),
                    None => Decoded::Invalid(format!("\\{esc}{value:x}")),
                },
                None => Decoded::Literal(format!("\\{esc}")),
            }
        }
        'c' => read_control(chars, raw),
        other => Decoded::Literal(format!("\\{other}")),
    }
}

fn read_hex(chars: &mut Peekable<Chars<'_>>, raw: &mut String, max: usize) -> Option<u32> {
    let mut value: Option<u32> = None;
    for _ in 0..max {
        let Some(digit) = chars.peek().and_then(|c| c.to_digit(16)) else {
            break;
        };
        value = Some(value.unwrap_or(0) * 16 + digit);
        if let Some(c) = chars.next() {
            raw.push(c);
        }
    }
    value
}

fn read_control(chars: &mut Peekable<Chars<'_>>, raw: &mut String) -> Decoded {
    match chars.peek().copied() {
        Some(c) if c.is_ascii() && c != '\'' => {
            chars.next();
            raw.push(c);
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
                        push_char(&mut bytes, paired);
                    }
                    None => {}
                }
            }
            Decoded::Bytes(bytes)
        }
        _ => Decoded::Invalid("\\c".to_string()),
    }
}

#[cfg(test)]
#[path = "../tests/ansi_c_tests.rs"]
mod tests;
