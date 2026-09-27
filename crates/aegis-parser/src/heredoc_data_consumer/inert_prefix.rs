//! Whether the command text before a heredoc marker can change which program
//! a bare `cat` or `jq` runs (ADR-042, PR #463 review round 3). The earlier
//! check searched the prefix for denylisted words (`alias`, `hash`, `PATH`,
//! ...), and each review found a shell table it missed: `BASH_CMDS[cat]=`,
//! `BASH_ALIASES[cat]=`, zsh `functions[cat]=` and `commands[cat]=`, zsh
//! `path=(...)`. This module inverts the check. Every simple command in the
//! prefix must start with a literal word from [`INERT_PREFIX_PROGRAMS`], and
//! no expansion in the prefix may assign a variable. Anything else, known or
//! not, costs the body a scan.

use super::DATA_CONSUMER_PROGRAMS;

/// Programs that cannot change the invoking shell's own state: external
/// programs run in a child process, and these builtins only print or change
/// the working directory. A shell function, alias, hash entry, or search path
/// can only change through a builtin or an assignment, and neither is here.
/// [`DATA_CONSUMER_PROGRAMS`] is accepted as well.
const INERT_PREFIX_PROGRAMS: &[&str] = &[
    "cd", "echo", "false", "gh", "git", "ls", "mkdir", "pwd", "true",
];

/// How far the walk is into the command word of the current simple command.
#[derive(Clone, Copy)]
enum CommandWord {
    /// No character of the command word yet.
    Pending,
    /// The command word started at this byte index.
    Started(usize),
    /// The command word is complete and passed the allowlist.
    Checked,
    /// The command word was `set`; its arguments start at this byte index
    /// and are checked by [`set_args_are_inert`] when the command ends.
    SetArgs(usize),
}

/// What opened a nested command list.
#[derive(PartialEq, Eq)]
enum Nest {
    /// `$(`.
    Command,
    /// A backtick.
    Backtick,
    /// Any other `(`: a subshell, process substitution, or zsh glob qualifier.
    Paren,
}

/// `true` when every simple command in `text` starts with a literal word from
/// [`INERT_PREFIX_PROGRAMS`] or [`DATA_CONSUMER_PROGRAMS`]. Arguments are not
/// checked here; [`has_assigning_expansion`] and [`has_expanding_heredoc`]
/// cover the expansions an argument can hold.
///
/// A command word built by an expansion (`$(echo eval) git '; x'`, `$c`) is
/// refused, since the walk cannot know what it runs. So is a `#` comment in
/// command position: the walk would still read quotes and separators inside
/// it that the shell ignores.
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
            // `>&2`, `<&0`, `&>f` and `>|f` are redirections, not separators.
            '&' | '|' if matches!(prev, Some('>' | '<')) => {}
            '&' if chars.peek().map(|&(_, c)| c) == Some('>') => {}
            ';' | '\n' | '&' | '|' => {
                if !finish_command(text, &mut word, idx) {
                    return false;
                }
                word = CommandWord::Pending;
            }
            // A standalone `{` opens a group whose first word is a command.
            // A standalone `}` closes one, and the next word is a command
            // again: zsh reads `} always { ... }` as a second group.
            '{' | '}'
                if matches!(word, CommandWord::Pending | CommandWord::Checked)
                    && chars
                        .peek()
                        .is_none_or(|&(_, c)| matches!(c, ' ' | '\t' | '\n' | ';' | ')')) =>
            {
                word = CommandWord::Pending;
            }
            // Only a blank separates words; bash and zsh do not split on
            // other Unicode whitespace.
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

/// Mark the command word as started at `idx` if none has started yet.
fn begin_word(word: &mut CommandWord, idx: usize) {
    if matches!(word, CommandWord::Pending) {
        *word = CommandWord::Started(idx);
    }
}

/// Close a started command word at `end` and check it against the allowlist.
/// Quote characters are dropped first (`"git"` runs `git`); a backslash or a
/// `$` stays, so an escaped or expanded word never matches.
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

/// Close the current simple command at `end`: finish its command word, then
/// check the arguments of a `set` command.
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

/// `true` when `args` is a plain error-handling option list for `set`
/// (`-euo pipefail`, `-x`, `+e`). Every other `set` form stays refused:
/// zsh `set -A name` assigns an array, `set -a` exports every later
/// assignment, and `set -k` moves assignments into a command's environment.
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

/// `true` when `text` holds an expansion that can assign a variable or run
/// arithmetic: `${...}` (`${PATH:=x}`, zsh `${path::=x}`), `$((...))`,
/// `$[...]`, a subscript (`$a[PATH=0]` is arithmetic in zsh), or a `$"..."`
/// locale string. `$(...)`, `$name`, and the special parameters stay allowed.
/// Quotes are ignored, so a quoted `${` only costs a scanned body.
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
                // zsh flag characters, then the name, then a subscript.
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

/// `true` when `text` holds a heredoc operator whose delimiter is not quoted.
/// Its body is left out of the prefix, and the shell expands that body: fed to
/// a builtin (`: <<X` with `$((PATH=0))` in the body), the expansion runs in
/// the current shell. `<<<` here-strings are skipped; their word sits on the
/// line itself, where [`has_assigning_expansion`] sees it.
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
