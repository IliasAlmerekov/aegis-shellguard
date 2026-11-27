use crate::split_tokens;
use crate::tokenizer::quote_scan::{Scan, SubstitutionScan};

mod inert_prefix;
mod message_flags;
mod quotes;

const DATA_CONSUMER_PROGRAMS: &[&str] = &[
    "cat",
    "jq",
    "/bin/cat",
    "/bin/jq",
    "/usr/bin/cat",
    "/usr/bin/jq",
];

fn is_bare_assignment(token: &str) -> bool {
    token
        .split_once('=')
        .is_some_and(|(name, _)| is_plain_identifier(name))
}

fn is_plain_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn owning_simple_command_start(text: &str) -> usize {
    let mut scan = SubstitutionScan::default();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();

    while let Some((idx, ch)) = chars.next() {
        match scan.step(idx, ch, &mut chars) {
            Scan::CommandStart(after_opener) => start = after_opener,
            Scan::Consumed | Scan::Quoted => {}
            Scan::Plain => match ch {
                ';' | '\n' => start = idx + 1,
                '&' | '|' if chars.peek().map(|&(_, c)| c) == Some(ch) => {
                    chars.next();
                    start = idx + 2;
                }
                _ => {}
            },
        }
    }
    start
}

fn owning_command_program(line: &str, owning_start: usize, marker_start: usize) -> Option<String> {
    let tokens = split_tokens(line[owning_start..marker_start].trim());
    tokens
        .into_iter()
        .next()
        .filter(|token| !is_bare_assignment(token))
}

fn is_data_consumer_program(program: &str) -> bool {
    DATA_CONSUMER_PROGRAMS.contains(&program)
}

fn tail_may_leave_plain_output(tail: &str) -> bool {
    tail.contains(['|', '\'', '"', '`', '$'])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameKind {
    Command,
    Backtick,
    Paren,
    Brace,
    Comment,
}

struct OpenFrame {
    start: usize,
    kind: FrameKind,
}

fn open_frames(text: &str) -> Vec<OpenFrame> {
    struct Frame {
        start: usize,
        kind: FrameKind,
        saved_quotes: (bool, bool),
    }

    let mut single_quote = false;
    let mut double_quote = false;
    let mut frames: Vec<Frame> = Vec::new();
    let mut chars = text.char_indices().peekable();

    while let Some((idx, ch)) = chars.next() {
        match ch {
            '\\' if !single_quote => {
                chars.next();
            }
            '\'' if !double_quote => single_quote = !single_quote,
            '"' if !single_quote => double_quote = !double_quote,
            '`' if !single_quote => {
                if matches!(frames.last(), Some(f) if f.kind == FrameKind::Backtick) {
                    if let Some(frame) = frames.pop() {
                        (single_quote, double_quote) = frame.saved_quotes;
                    }
                } else {
                    frames.push(Frame {
                        start: idx,
                        kind: FrameKind::Backtick,
                        saved_quotes: (single_quote, double_quote),
                    });
                    single_quote = false;
                    double_quote = false;
                }
            }
            '$' if !single_quote && chars.peek().map(|&(_, c)| c) == Some('(') => {
                chars.next();
                frames.push(Frame {
                    start: idx,
                    kind: FrameKind::Command,
                    saved_quotes: (single_quote, double_quote),
                });
                single_quote = false;
                double_quote = false;
            }
            '(' if !single_quote && !double_quote => {
                frames.push(Frame {
                    start: idx,
                    kind: FrameKind::Paren,
                    saved_quotes: (single_quote, double_quote),
                });
            }
            ')' if !single_quote && !double_quote => {
                if matches!(
                    frames.last(),
                    Some(f) if matches!(f.kind, FrameKind::Command | FrameKind::Paren)
                ) && let Some(frame) = frames.pop()
                {
                    (single_quote, double_quote) = frame.saved_quotes;
                }
            }
            '{' if !single_quote
                && !double_quote
                && starts_word(text, idx)
                && chars.peek().is_none_or(|&(_, c)| c.is_whitespace()) =>
            {
                frames.push(Frame {
                    start: idx,
                    kind: FrameKind::Brace,
                    saved_quotes: (single_quote, double_quote),
                });
            }
            '#' if !single_quote && !double_quote && starts_word(text, idx) => {
                frames.push(Frame {
                    start: idx,
                    kind: FrameKind::Comment,
                    saved_quotes: (single_quote, double_quote),
                });
            }
            '}' if !single_quote && !double_quote && starts_word(text, idx) => {
                if matches!(frames.last(), Some(f) if f.kind == FrameKind::Brace) {
                    frames.pop();
                }
            }
            _ => {}
        }
    }

    frames
        .into_iter()
        .map(|f| OpenFrame {
            start: f.start,
            kind: f.kind,
        })
        .collect()
}

fn starts_word(text: &str, idx: usize) -> bool {
    text[..idx]
        .chars()
        .next_back()
        .is_none_or(|c| c.is_whitespace() || matches!(c, ';' | '&' | '|' | '('))
}

const UNTRUSTED_PREFIX_WORDS: &[&str] = &[
    "case", "coproc", "do", "elif", "else", "exec", "for", "function", "if", "select", "then",
    "until", "while",
];

fn shell_words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
}

fn defines_function(prefix: &str) -> bool {
    let mut compact = prefix.chars().filter(|c| !c.is_whitespace()).peekable();
    while let Some(ch) = compact.next() {
        if ch == '(' && compact.peek() == Some(&')') {
            return true;
        }
    }
    false
}

fn writes_to_open_descriptor(line: &str) -> bool {
    if line.contains("/dev/fd/") || line.contains("/proc/") {
        return true;
    }
    line.match_indices(">&").any(|(idx, _)| {
        let rest = &line[idx + 2..];
        if rest.starts_with('$') {
            return true;
        }
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        !digits.is_empty() && digits != "1" && digits != "2"
    })
}

fn consumer_writes_file(command: &str) -> bool {
    let mut single_quote = false;
    let mut double_quote = false;
    let mut chars = command.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' if !single_quote => {
                chars.next();
            }
            '\'' if !double_quote => single_quote = !single_quote,
            '"' if !single_quote => double_quote = !double_quote,
            '>' if !single_quote && !double_quote => {
                if chars.peek() != Some(&'&') {
                    return true;
                }
                chars.next();
                if !chars.peek().is_some_and(char::is_ascii_digit) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

#[derive(Debug, PartialEq, Eq)]
enum HeredocMarkerContext {
    TopLevel,
    TrustedCommandArg,
    Untrusted,
}

fn is_trusted_message_value(owning: &str) -> bool {
    let tokens = split_tokens(owning);
    let Some(program_pos) = tokens.iter().position(|token| !is_bare_assignment(token)) else {
        return false;
    };
    if tokens.len() < program_pos + 2 {
        return false;
    }
    let program = &tokens[program_pos];
    let basename = program
        .rsplit_once('/')
        .map_or(program.as_str(), |(_, tail)| tail);
    let is_git = basename.eq_ignore_ascii_case("git");
    let is_gh = basename.eq_ignore_ascii_case("gh");
    if !is_git && !is_gh {
        return false;
    }
    let last = &tokens[tokens.len() - 1];
    let context_args = &tokens[program_pos + 1..tokens.len() - 1];
    let flag = if owning.ends_with('=') {
        let stripped = last.strip_suffix('=').unwrap_or(last.as_str());
        if !stripped.starts_with("--") {
            return false;
        }
        stripped
    } else if owning.ends_with(char::is_whitespace) {
        last.as_str()
    } else {
        return false;
    };
    if is_git {
        message_flags::git_message_flag_trusted(context_args, flag)
    } else {
        message_flags::gh_message_flag_trusted(context_args, flag)
    }
}

fn heredoc_marker_context(prefix: &str) -> HeredocMarkerContext {
    if prefix.contains("$'") {
        return HeredocMarkerContext::Untrusted;
    }
    let unquoted: String = prefix
        .chars()
        .filter(|c| !matches!(c, '\'' | '"' | '\\'))
        .collect();
    if shell_words(&unquoted).any(|word| UNTRUSTED_PREFIX_WORDS.contains(&word))
        || defines_function(prefix)
        || inert_prefix::has_assigning_expansion(prefix)
        || inert_prefix::has_expanding_heredoc(prefix)
    {
        return HeredocMarkerContext::Untrusted;
    }
    let frames = open_frames(prefix);
    let frame = match frames.as_slice() {
        [] if quotes::quote_open_at_end(prefix) => return HeredocMarkerContext::Untrusted,
        [] if !inert_prefix::every_command_is_inert(prefix) => {
            return HeredocMarkerContext::Untrusted;
        }
        [] => return HeredocMarkerContext::TopLevel,
        [frame] => frame,
        _ => return HeredocMarkerContext::Untrusted,
    };
    if frame.kind != FrameKind::Command {
        return HeredocMarkerContext::Untrusted;
    }

    let mut preceding = &prefix[..frame.start];
    if let Some(stripped) = preceding
        .strip_suffix('"')
        .or_else(|| preceding.strip_suffix('\''))
    {
        preceding = stripped;
    }
    let command_start = owning_simple_command_start(preceding);
    let owning = preceding[command_start..].trim_start();

    if is_trusted_message_value(owning) && inert_prefix::every_command_is_inert(prefix) {
        return HeredocMarkerContext::TrustedCommandArg;
    }

    HeredocMarkerContext::Untrusted
}

pub(super) fn heredoc_target_is_data_consumer(
    preceding_lines: &str,
    line: &str,
    marker_start: usize,
    delimiter_end: usize,
) -> bool {
    let marker_prefix = &line[..marker_start];
    let owning_start = owning_simple_command_start(marker_prefix);
    let program = owning_command_program(line, owning_start, marker_start);

    if !program.as_deref().is_some_and(is_data_consumer_program)
        || tail_may_leave_plain_output(&line[delimiter_end..])
        || line.contains(">(")
        || line.contains("<(")
        || line.trim_end().ends_with('\\')
        || writes_to_open_descriptor(line)
    {
        return false;
    }
    if consumer_writes_file(&line[owning_start..]) {
        return false;
    }
    let prefix = if preceding_lines.is_empty() {
        std::borrow::Cow::Borrowed(marker_prefix)
    } else {
        std::borrow::Cow::Owned(format!("{preceding_lines}{marker_prefix}"))
    };
    match heredoc_marker_context(&prefix) {
        HeredocMarkerContext::TopLevel | HeredocMarkerContext::TrustedCommandArg => true,
        HeredocMarkerContext::Untrusted => false,
    }
}

#[cfg(test)]
#[path = "tests/owning_start_tests.rs"]
mod owning_start_tests;
