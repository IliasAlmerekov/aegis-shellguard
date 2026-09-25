//! The `Data consumer` predicate for a nowdoc heredoc body (issue #396,
//! #432): whether `cat`, `tee`, or `jq` owns the marker, whether anything
//! pipes the consumer's own stdout, and whether the marker sits in a context
//! [`heredoc_target_is_data_consumer`] trusts (ADR-040). Split out of
//! [`super::embedded_scripts`] to keep that file under the 800-line budget
//! in `tests/file_size_budget.rs`; `embedded_scripts` is this predicate's
//! only caller.

use crate::split_tokens;

/// Programs that never execute their stdin: `cat`/`tee` copy the bytes to
/// stdout or a file, and `jq` parses them as JSON. Their output only reaches
/// a shell through a pipe, a process substitution, or an enclosing
/// substitution, which the rest of the predicate rules out. A nowdoc body
/// fed only to one of these is `Data consumer` territory (issue #396, #432)
/// — see [`heredoc_target_is_data_consumer`] for the full predicate,
/// including the pipe and context checks a bare program match alone doesn't
/// cover. `sed` (`e` command) and `awk` (`system()`) are deliberately not on
/// this list: both can run shell commands from their own script argument.
const DATA_CONSUMER_PROGRAMS: &[&str] = &["cat", "tee", "jq"];

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

/// Byte index in `text` right after the last unquoted occurrence of `;`,
/// `&&`, `||`, `(`, `$(`, or a backtick — the boundary the simple command
/// that owns a same-line heredoc marker starts at (issue #432: `true; cat >
/// f <<'EOF'` must resolve to the command after `;`, not the line's own
/// first token). `0` when none exists, so an unbroken line still resolves to
/// its own first token exactly as before.
///
/// `$(`/backtick each open a fresh quote scope even inside an outer
/// double-quoted string, mirroring [`open_frames`]'s own nesting
/// rules — required so `gh pr create --body "$(cat <<'EOF'` resolves its
/// owning command to `cat`, not to a still-open outer double quote hiding
/// every later boundary from view (issue #396).
///
/// A single `|` is deliberately not a boundary here: a heredoc's own
/// redirection overrides whatever a preceding pipe stage would have fed its
/// stdin, so the consumer is still the word right after the marker's owning
/// command — but leaving `|` unrecognized as a boundary means a lone pipe
/// stays glued to whatever precedes it, which is the conservative
/// (not-inert) outcome whenever that combination cannot otherwise be told
/// apart (issue #396).
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

/// The basename-normalized program of the simple command that owns the
/// heredoc marker at `marker_start` on `line`, skipping a leading run of
/// bare `NAME=value` assignments. `None` when the owning text has no
/// program token at all (an empty prefix, or one made only of assignments).
fn heredoc_owning_command_program(line: &str, marker_start: usize) -> Option<String> {
    let prefix = &line[..marker_start];
    let command_start = owning_simple_command_start(prefix);
    let tokens = split_tokens(prefix[command_start..].trim());
    let program = tokens.iter().find(|token| !is_bare_assignment(token))?;
    let basename = program
        .rsplit_once('/')
        .map_or(program.as_str(), |(_, tail)| tail);
    Some(basename.to_string())
}

/// `true` when the simple command that owns the heredoc marker at
/// `marker_start` on `line` invokes a [`DATA_CONSUMER_PROGRAMS`] member.
fn heredoc_owning_command_is_data_consumer(line: &str, marker_start: usize) -> bool {
    heredoc_owning_command_program(line, marker_start).is_some_and(|program| {
        DATA_CONSUMER_PROGRAMS
            .iter()
            .any(|candidate| program.eq_ignore_ascii_case(candidate))
    })
}

/// `true` when `tail` (the text on a heredoc marker's own line, past its
/// delimiter spec) carries an unquoted `|` — a same-line pipe out of the
/// heredoc's consumer (issue #396: `jq -r .a <<'JSON' | sh` must not be
/// treated as inert, since `sh` reads whatever `jq` prints).
fn tail_has_pipe(tail: &str) -> bool {
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut chars = tail.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' if !in_single_quote => {
                chars.next();
            }
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            '|' if !in_single_quote && !in_double_quote => return true,
            _ => {}
        }
    }
    false
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

/// Every `$(`, backtick, `(`, `{` group and `#` comment frame still open at
/// the end of `text`, outermost first. A comment frame never closes. `$(` and a backtick each open a fresh quote scope even
/// inside a double-quoted string, the same rule
/// `embedded_scripts::find_unquoted_double_lt` follows, and restore the
/// enclosing scope when they close.
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

/// The frame a heredoc marker sits in. [`heredoc_target_is_data_consumer`]
/// trusts only the first three (issue #396, #432).
#[derive(Debug, PartialEq, Eq)]
enum HeredocMarkerContext {
    /// No `$(`, backtick, `(`, `{` or comment frame is open before the marker.
    TopLevel,
    /// Inside exactly one `$(...)`, itself the right-hand side of a plain
    /// `NAME=` assignment.
    AssignmentRhs,
    /// Inside exactly one `$(...)`, itself the whole value of a message flag
    /// (see [`MESSAGE_FLAGS`]) of a `git` or `gh` invocation.
    TrustedCommandArg,
    /// Anything else: nested frames, a backtick, a subshell or process
    /// substitution `(`, an open `{` group, a `#` comment, an [`UNTRUSTED_PREFIX_WORDS`] word
    /// before the marker, or a `$(...)` that is
    /// not a `git`/`gh` message value — a `bash -c
    /// "$(...)"`, `eval "$(...)"`, `ssh host "$(...)"`, `echo "$(...)" | sh`,
    /// `git -c "alias.x=!$(...)"` or `gh alias set --shell x "$(...)"` shape
    /// among them.
    Untrusted,
}

/// Flags whose value `git` and `gh` only ever store or send as text: a commit
/// or tag message, an issue or PR title or body. Any other `git`/`gh`
/// argument may name something they run (`git -c alias.x=!cmd`, `gh alias
/// set --shell`), so it is not trusted.
const MESSAGE_FLAGS: &[&str] = &["-m", "--message", "-t", "--title", "-b", "--body"];

/// `true` when `owning` (the simple command text right before an enclosing
/// `$(`, its opening quote already stripped) is a `git` or `gh` invocation
/// whose last word is a [`MESSAGE_FLAGS`] member, either standalone
/// (`git commit -m `) or glued to its value (`gh pr create --body=`).
fn is_trusted_message_value(owning: &str) -> bool {
    let tokens = split_tokens(owning);
    let Some(program) = tokens.iter().find(|token| !is_bare_assignment(token)) else {
        return false;
    };
    let basename = program
        .rsplit_once('/')
        .map_or(program.as_str(), |(_, tail)| tail);
    if !basename.eq_ignore_ascii_case("git") && !basename.eq_ignore_ascii_case("gh") {
        return false;
    }
    let Some(last) = tokens.last() else {
        return false;
    };
    if owning.ends_with('=') {
        let flag = last.strip_suffix('=').unwrap_or(last);
        return flag.starts_with("--") && MESSAGE_FLAGS.contains(&flag);
    }
    // A standalone flag must be followed by the substitution itself, not
    // glued to it: `owning` then ends in whitespace after the flag.
    owning.ends_with(char::is_whitespace) && MESSAGE_FLAGS.contains(&last.as_str())
}

/// Classify a heredoc marker by [`HeredocMarkerContext`]. `prefix` is every
/// command line before the marker's own line (heredoc bodies left out, see
/// `embedded_scripts::walk_heredocs`) followed by the marker line up to the
/// `<<`, so a `$(` opened on an earlier line still counts as enclosing.
fn heredoc_marker_context(prefix: &str) -> HeredocMarkerContext {
    // The check ignores quotes on purpose: a quoted keyword only costs a
    // scanned body, never a skipped one.
    if prefix
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|word| UNTRUSTED_PREFIX_WORDS.contains(&word))
    {
        return HeredocMarkerContext::Untrusted;
    }
    let frames = open_frames(prefix);
    let frame = match frames.as_slice() {
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

    if let Some(name) = owning.strip_suffix('=')
        && is_plain_identifier(name)
    {
        return HeredocMarkerContext::AssignmentRhs;
    }

    if is_trusted_message_value(owning) {
        return HeredocMarkerContext::TrustedCommandArg;
    }

    HeredocMarkerContext::Untrusted
}

/// `true` when the nowdoc body at the marker whose `<<` sits at
/// `marker_start`/ends its spec at `delimiter_end` on `line` is `Data
/// consumer` territory (issue #396, #432): fed only to `cat`, `tee`, or `jq`
/// as the program of the simple command that owns the marker, piped to
/// nothing else, not continued onto the next line by a trailing `\`, with no
/// process substitution on the marker line (`cat
/// <<'EOF' > >(sh)`, `tee >(sh) <<'EOF'` hand the body to a shell), no
/// write to a descriptor above 2 or a `/dev/fd/`/`/proc/` path, and reached
/// only through a context this predicate trusts (top level, an assignment's
/// `$(...)`, or a `git`/`gh` message value's `$(...)`, with no subshell or
/// process-substitution `(` or `{` group still open and no `#` comment or
/// [`UNTRUSTED_PREFIX_WORDS`] word before the marker).
/// `preceding_lines` holds the command lines before `line`, bodies left out.
/// Ignorant of nowdoc-ness itself — callers already gate on that
/// separately, matching how `embedded_scripts::heredoc_target_program`'s
/// interpreter check is computed unconditionally too.
pub(super) fn heredoc_target_is_data_consumer(
    preceding_lines: &str,
    line: &str,
    marker_start: usize,
    delimiter_end: usize,
) -> bool {
    if !heredoc_owning_command_is_data_consumer(line, marker_start)
        || tail_has_pipe(&line[delimiter_end..])
        || line.contains(">(")
        || line.contains("<(")
        // A trailing `\` continues the command onto the next line (`cat
        // <<'EOF' \` then `| sh`), which this line walk would misread as body.
        || line.trim_end().ends_with('\\')
        || writes_to_open_descriptor(line)
    {
        return false;
    }
    let marker_prefix = &line[..marker_start];
    let prefix = if preceding_lines.is_empty() {
        std::borrow::Cow::Borrowed(marker_prefix)
    } else {
        std::borrow::Cow::Owned(format!("{preceding_lines}{marker_prefix}"))
    };
    matches!(
        heredoc_marker_context(&prefix),
        HeredocMarkerContext::TopLevel
            | HeredocMarkerContext::AssignmentRhs
            | HeredocMarkerContext::TrustedCommandArg
    )
}
