use super::DATA_CONSUMER_PROGRAMS;

const INERT_PREFIX_PROGRAMS: &[&str] = &[
    "cd", "echo", "false", "gh", "git", "ls", "mkdir", "pwd", "true",
];

#[derive(Clone, Copy)]
enum CommandWord {
    Pending,
    Started(usize),
    Checked,
    SetArgs(usize),
}

#[derive(PartialEq, Eq)]
enum Nest {
    Command,
    Backtick,
    Paren,
}

pub(super) fn every_command_is_inert(text: &str) -> bool {
    let mut single_quote = false;
    let mut double_quote = false;
    let mut word = CommandWord::Pending;
    let mut nests: Vec<(Nest, (bool, bool))> = Vec::new();
    let mut prev = None;
    let mut chars = text.char_indices().peekable();

    while let Some((idx, ch)) = chars.next() {
        match ch {
            _ if single_quote => {
                if ch == '\'' {
                    single_quote = false;
                }
            }
            '\\' => {
                begin_word(&mut word, idx);
                chars.next();
            }
            '\'' if !double_quote => {
                begin_word(&mut word, idx);
                single_quote = true;
            }
            '"' => {
                begin_word(&mut word, idx);
                double_quote = !double_quote;
            }
            '`' if matches!(nests.last(), Some((Nest::Backtick, _))) => {
                if !finish_command(text, &mut word, idx) {
                    return false;
                }
                if let Some((_, saved)) = nests.pop() {
                    (single_quote, double_quote) = saved;
                }
                word = CommandWord::Checked;
            }
            '`' | '$' if ch == '`' || chars.peek().map(|&(_, c)| c) == Some('(') => {
                if !matches!(word, CommandWord::Checked) {
                    return false;
                }
                let kind = if ch == '`' {
                    Nest::Backtick
                } else {
                    chars.next();
                    Nest::Command
                };
                nests.push((kind, (single_quote, double_quote)));
                single_quote = false;
                double_quote = false;
                word = CommandWord::Pending;
            }
            _ if double_quote => {}
            '(' => {
                if !finish_command(text, &mut word, idx) {
                    return false;
                }
                nests.push((Nest::Paren, (false, false)));
                word = CommandWord::Pending;
            }
            ')' => {
                if !finish_command(text, &mut word, idx) {
                    return false;
                }
                match nests.pop() {
                    Some((Nest::Command | Nest::Paren, saved)) => {
                        (single_quote, double_quote) = saved;
                    }
                    _ => return false,
                }
                word = CommandWord::Checked;
            }
            '#' if matches!(word, CommandWord::Pending) => return false,
            '&' | '|' if matches!(prev, Some('>' | '<')) => {}
            ';' | '\n' | '&' | '|' => {
                if !finish_command(text, &mut word, idx) {
                    return false;
                }
                word = CommandWord::Pending;
            }
            '{' | '}'
                if matches!(word, CommandWord::Pending | CommandWord::Checked)
                    && chars
                        .peek()
                        .is_none_or(|&(_, c)| matches!(c, ' ' | '\t' | '\n' | ';' | ')')) =>
            {
                word = CommandWord::Pending;
            }
            ' ' | '\t' => {
                if !finish_word(text, &mut word, idx) {
                    return false;
                }
            }
            _ => begin_word(&mut word, idx),
        }
        prev = Some(ch);
    }

    finish_command(text, &mut word, text.len())
}

fn begin_word(word: &mut CommandWord, idx: usize) {
    if matches!(word, CommandWord::Pending) {
        *word = CommandWord::Started(idx);
    }
}

fn finish_word(text: &str, word: &mut CommandWord, end: usize) -> bool {
    let CommandWord::Started(start) = *word else {
        return true;
    };
    let literal: String = text[start..end]
        .chars()
        .filter(|c| !matches!(c, '\'' | '"'))
        .collect();
    if literal == "set" {
        *word = CommandWord::SetArgs(end);
        return true;
    }
    *word = CommandWord::Checked;
    INERT_PREFIX_PROGRAMS.contains(&literal.as_str())
        || DATA_CONSUMER_PROGRAMS.contains(&literal.as_str())
}

fn finish_command(text: &str, word: &mut CommandWord, end: usize) -> bool {
    if !finish_word(text, word, end) {
        return false;
    }
    if let CommandWord::SetArgs(start) = *word {
        *word = CommandWord::Checked;
        return set_args_are_inert(&text[start..end]);
    }
    true
}

fn set_args_are_inert(args: &str) -> bool {
    const OPTION_NAMES: &[&str] = &["errexit", "nounset", "pipefail", "verbose", "xtrace"];
    let mut words = args.split_whitespace().peekable();
    while let Some(arg) = words.next() {
        let Some(letters) = arg.strip_prefix(['-', '+']) else {
            return false;
        };
        if letters.is_empty()
            || !letters
                .chars()
                .all(|c| matches!(c, 'e' | 'u' | 'v' | 'x' | 'o'))
        {
            return false;
        }
        for _ in letters.matches('o') {
            if !words
                .next()
                .is_some_and(|name| OPTION_NAMES.contains(&name))
            {
                return false;
            }
        }
    }
    true
}

pub(super) fn has_assigning_expansion(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.iter().enumerate().any(|(idx, &b)| {
        if b != b'$' {
            return false;
        }
        let rest = &bytes[idx + 1..];
        match rest.first() {
            Some(b'(') => rest.get(1) == Some(&b'('),
            Some(b'{' | b'[' | b'"' | b'\'') => true,
            Some(_) => {
                let after_flags = rest
                    .iter()
                    .position(|c| !matches!(c, b'=' | b'~' | b'^' | b'+' | b'#'))
                    .unwrap_or(rest.len());
                let after_name = rest[after_flags..]
                    .iter()
                    .position(|c| !(c.is_ascii_alphanumeric() || *c == b'_'))
                    .map_or(rest.len(), |pos| after_flags + pos);
                rest.get(after_name) == Some(&b'[')
            }
            None => false,
        }
    })
}

pub(super) fn has_expanding_heredoc(text: &str) -> bool {
    text.match_indices("<<").any(|(idx, _)| {
        let before = text[..idx].chars().next_back();
        let rest = &text[idx + 2..];
        if before == Some('<') || rest.starts_with('<') {
            return false;
        }
        let delimiter = rest.trim_start_matches('-').trim_start_matches([' ', '\t']);
        !delimiter.starts_with(['\'', '"'])
    })
}
