//! `Data consumer` predicate for nowdoc heredoc bodies (ADR-042, #396, #432).

use crate::split_tokens;

mod inert_prefix;
mod message_flags;
mod quotes;

/// Programs that never execute their stdin (`cat`, `jq`) and their stock paths.
///
/// `tee` is excluded because it writes a file; `sed` and `awk` can execute commands;
/// non-stock paths (e.g. `./cat`, `/tmp/p/cat`) may name a copied shell (#396, #432).
const DATA_CONSUMER_PROGRAMS: &[&str] = &[
    "cat",
    "jq",
    "/bin/cat",
    "/bin/jq",
    "/usr/bin/cat",
    "/usr/bin/jq",
];

/// `true` when `token` is a bare `NAME=value` shell-variable assignment —
/// used to skip a leading assignment run ahead of a heredoc's owning program
/// (`FOO=1 cat <<'EOF'` still resolves to `cat`).
fn is_bare_assignment(token: &str) -> bool {
    token
        .split_once('=')
        .is_some_and(|(name, _)| is_plain_identifier(name))
}

/// `true` when `name` is a non-empty ASCII shell identifier: a leading
/// letter or underscore, then only alphanumerics or underscores.
fn is_plain_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Byte index in `text` right after the last unquoted `;`, `&&`, `||`, `(`, `$(`, or backtick.
///
/// `$(` and backticks open a fresh quote scope even inside double quotes (#396, #432).
/// A single `|` is not a boundary so a piped prefix stays conservative (not inert).
fn owning_simple_command_start(text: &str) -> usize {
    struct Frame {
        command_sub_depth: Option<usize>,
        saved_quotes: (bool, bool),
    }

    let mut single_quote = false;
    let mut double_quote = false;
    let mut frames: Vec<Frame> = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();

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
                    frames.push(Frame {
                        command_sub_depth: None,
                        saved_quotes: (single_quote, double_quote),
                    });
                    single_quote = false;
                    double_quote = false;
                    start = idx + ch.len_utf8();
                }
            }
            '$' if !single_quote && chars.peek().map(|&(_, c)| c) == Some('(') => {
                if let Some((paren_idx, paren_ch)) = chars.next() {
                    frames.push(Frame {
                        command_sub_depth: Some(1),
                        saved_quotes: (single_quote, double_quote),
                    });
                    single_quote = false;
                    double_quote = false;
                    start = paren_idx + paren_ch.len_utf8();
                }
            }
            '(' if !single_quote && !double_quote => {
                if let Some(depth) = frames.last_mut().and_then(|f| f.command_sub_depth.as_mut()) {
                    *depth += 1;
                } else {
                    start = idx + ch.len_utf8();
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
            ';' | '\n' if !single_quote && !double_quote => {
                start = idx + ch.len_utf8();
            }
            '&' | '|'
                if !single_quote && !double_quote && chars.peek().map(|&(_, c)| c) == Some(ch) =>
            {
                if let Some((next_idx, next_ch)) = chars.next() {
                    start = next_idx + next_ch.len_utf8();
                }
            }
            _ => {}
        }
    }
    start
}

/// Program token of the simple command in `line[owning_start..marker_start]`.
///
/// Returns `None` if it starts with a `NAME=value` assignment because overriding
/// `PATH` can replace `cat` or `jq` with a shell.
fn owning_command_program(line: &str, owning_start: usize, marker_start: usize) -> Option<String> {
    let tokens = split_tokens(line[owning_start..marker_start].trim());
    tokens
        .into_iter()
        .next()
        .filter(|token| !is_bare_assignment(token))
}

/// `true` when `program` is an exact, case-sensitive [`DATA_CONSUMER_PROGRAMS`] member.
fn is_data_consumer_program(program: &str) -> bool {
    DATA_CONSUMER_PROGRAMS.contains(&program)
}

/// `true` when `tail` past the delimiter spec may pipe output or continue the command past newline.
///
/// Ignores quote state so false positives only cost a scanned body.
fn tail_may_leave_plain_output(tail: &str) -> bool {
    tail.contains(['|', '\'', '"', '`', '$'])
}

/// What opened a frame that is still open at a heredoc marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameKind {
    /// `$(`: command substitution.
    Command,
    /// A backtick command substitution.
    Backtick,
    /// Any other `(`: a subshell, a process substitution `<(`/`>(`, or an
    /// arithmetic paren. Its output can reach a pipe or a shell after the
    /// heredoc's terminator line, where the marker line cannot see it.
    Paren,
    /// A `{ ...; }` group, which can pipe or redirect its output after the
    /// terminator line the same way a subshell can (`} | sh`, `} >&3`).
    Brace,
    /// A `#` comment. The shell ignores the rest of its line, but this
    /// scanner would still read a `}` or `)` there as a close, so a comment
    /// before the marker never closes and keeps the context untrusted.
    Comment,
}

/// One frame still open at the end of a scanned prefix, with where it
/// started so a caller can inspect the text right before it.
struct OpenFrame {
    start: usize,
    kind: FrameKind,
}

/// Every `$(`, backtick, `(`, `{` group, and `#` comment frame still open at the end of `text`.
///
/// Comment frames never close; `$(` and backticks open a fresh quote scope even inside double quotes.
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
            // `{` and `}` are reserved words, so only a standalone one opens
            // or closes a group: `a{b,c}` and `${x}` do neither.
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

/// `true` when the character at `idx` starts a shell word: it is the first
/// character of `text` or follows whitespace or a command separator.
fn starts_word(text: &str, idx: usize) -> bool {
    text[..idx]
        .chars()
        .next_back()
        .is_none_or(|c| c.is_whitespace() || matches!(c, ';' | '&' | '|' | '('))
}

/// Words that make every later heredoc marker untrusted. Most are reserved
/// words that open a compound command or a coprocess: the output of a
/// command inside one can leave through a pipe or redirect written after the
/// heredoc's terminator line (`done | sh`, `fi >&3`), and a `case` pattern's
/// `)` closes nothing, so it would pop the frame that really encloses the
/// marker. `exec` can point stdout itself at a shell (`exec > >(sh)`), so a
/// later consumer with no redirect of its own still feeds one.
const UNTRUSTED_PREFIX_WORDS: &[&str] = &[
    "case", "coproc", "do", "elif", "else", "exec", "for", "function", "if", "select", "then",
    "until", "while",
];

/// Every maximal run of ASCII letters, digits, and `_` in `text` — the same
/// notion of a shell word [`is_plain_identifier`] validates one word at a
/// time, applied here so a caller can search a whole line for a specific
/// reserved word or builtin name (`printf`, `PATH`, ...) without also
/// matching it as a substring of something else (`PATHS`, `unprintf`).
fn shell_words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
}

/// `true` when `prefix` may define a POSIX shell function (which can shadow `cat`/`jq`).
///
/// Ignores quotes so a quoted `()` only costs a scanned body.
fn defines_function(prefix: &str) -> bool {
    let mut compact = prefix.chars().filter(|c| !c.is_whitespace()).peekable();
    while let Some(ch) = compact.next() {
        if ch == '(' && compact.peek() == Some(&')') {
            return true;
        }
    }
    false
}

/// `true` when `line` sends the consumer's output somewhere other than a
/// file, the terminal or stderr: a descriptor above 2 or a variable one
/// (`>&3`, `>&$fd`), or a `/dev/fd/` or `/proc/` path. Any of these can be a
/// pipe to a shell opened earlier (`exec 3> >(sh)`).
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

/// `true` when `command` (the owning simple command through the end of its
/// marker line) writes a file: it has an unquoted `>` that is not a
/// `>&1`/`>&2` duplication. Descriptors above 2 are already refused by
/// [`writes_to_open_descriptor`].
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

/// The frame a heredoc marker sits in. [`heredoc_target_is_data_consumer`]
/// trusts only the first two (#396, #432).
#[derive(Debug, PartialEq, Eq)]
enum HeredocMarkerContext {
    /// No `$(`, backtick, `(`, `{` or comment frame is open before the marker.
    TopLevel,
    /// Inside exactly one `$(...)`, itself the whole value of a message flag
    /// (see [`is_trusted_message_value`]) of a `git` or `gh` invocation.
    TrustedCommandArg,
    /// Anything else: nested frames, non-message substitutions, compound
    /// prefixes, or `NAME=$(...)` captures (ADR-042).
    Untrusted,
}

/// `true` when `owning` (simple command before `$(`, opening quote stripped)
/// ends with a trusted `git`/`gh` message flag for its subcommand (#396).
fn is_trusted_message_value(owning: &str) -> bool {
    let tokens = split_tokens(owning);
    let Some(program_pos) = tokens.iter().position(|token| !is_bare_assignment(token)) else {
        return false;
    };
    // At least one more token — the flag itself — must follow the program.
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

/// Classify a heredoc marker by [`HeredocMarkerContext`]. `prefix` is every
/// command line before the marker's own line (heredoc bodies left out, see
/// `embedded_scripts::walk_heredocs`) followed by the marker line up to the
/// `<<`, so a `$(` opened on an earlier line still counts as enclosing.
fn heredoc_marker_context(prefix: &str) -> HeredocMarkerContext {
    // Bail on ANSI-C `$'` because the quote-toggling walks below do not decode `\'` escapes.
    if prefix.contains("$'") {
        return HeredocMarkerContext::Untrusted;
    }
    // Ignores quotes on purpose so quoted keywords only cost a scan; stripping
    // quotes also joins split words (`PA""TH`, `al'ias'`).
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
        // `open_frames` only tracks bracket frames; reject unclosed quotes from earlier lines.
        [] if quotes::quote_open_at_end(prefix) => return HeredocMarkerContext::Untrusted,
        // Which program `cat` runs depends on every command before it
        // (ADR-042), so each one must be a known inert command.
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

/// `true` when the heredoc marker on `line` is owned by a trusted data consumer
/// (`cat` or `jq`) with no pipe, file write, descriptor > 2, process
/// substitution, line continuation, or untrusted enclosing context (ADR-042, #396, #432).
///
/// Callers gate on nowdoc-ness separately.
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
        // A trailing `\` continues the command onto the next line (`cat
        // <<'EOF' \` then `| sh`), which this line walk would misread as body.
        || line.trim_end().ends_with('\\')
        || writes_to_open_descriptor(line)
    {
        return false;
    }
    // A written file may run implicitly on a later invocation (for example,
    // a git hook or shell startup file). Its path cannot prove it is inert.
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
