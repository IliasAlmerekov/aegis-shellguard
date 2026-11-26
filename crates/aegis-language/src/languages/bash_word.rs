use std::iter::Peekable;
use std::str::Chars;

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
                Some('\n') | None => {}
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

#[derive(Default)]
struct Words {
    current: String,
    first: Option<String>,
    count: usize,
}

impl Words {
    fn split(&mut self) {
        if !self.current.is_empty() {
            let word = std::mem::take(&mut self.current);
            self.push(word);
        }
    }

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

fn ends_with_redirect(current: &str) -> bool {
    let mut rev = current.trim_end().chars().rev();
    match rev.next() {
        Some('>' | '<') => rev.take_while(|&c| c == '\\').count() % 2 == 0,
        _ => false,
    }
}

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

fn take_into(chars: &mut Peekable<Chars<'_>>, raw: &mut String) {
    if let Some(c) = chars.next() {
        raw.push(c);
    }
}

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
