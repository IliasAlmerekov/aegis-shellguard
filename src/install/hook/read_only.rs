//! Recognise `aegis` commands the `Hook` may wrap instead of deny.
//!
//! A read-only `aegis` command may appear alone, chained with other read-only
//! `aegis` commands (`&&`, `||`, `;`, newline), or piped into a text filter.
//! Its output may go to `/dev/null` or to another descriptor (`2>&1`). Every
//! other shape stays denied, because it could carry a command that changes
//! Aegis itself (#333, #379).

/// Programs a read-only `aegis` command may pipe into. None of them writes a
/// file or runs a command, so a pipeline cannot turn read-only output into a
/// change to Aegis. `sort -o`, `uniq IN OUT`, `tee`, and `sh` would.
pub(super) const PIPE_FILTERS: [&str; 6] = ["head", "tail", "grep", "wc", "jq", "cut"];

/// True when every command in `command` is a read-only `aegis` invocation or a
/// `PIPE_FILTERS` program that reads a pipe, joined only by the operators and
/// redirections listed in the module docs.
///
/// Quotes, `$`, backticks, parentheses, and globs are rejected before any
/// splitting, so the split below never has to reason about quoting or
/// substitution, and a read-only prefix cannot hide a second command.
pub(super) fn is_read_only_aegis_command(command: &str) -> bool {
    let Some(commands) = split_simple_commands(command) else {
        return false;
    };
    if commands[0].words.first() != Some(&"aegis") {
        return false;
    }
    let last = commands.len() - 1;
    commands
        .iter()
        .enumerate()
        .all(|(index, simple)| match simple.words.split_first() {
            Some((&"aegis", args)) => is_read_only_aegis_args(args),
            Some((program, _)) => simple.reads_pipe && PIPE_FILTERS.contains(program),
            // Only a trailing `;` or newline may leave an empty command.
            None => index == last && !simple.reads_pipe,
        })
}

/// True when `args` (the words after `aegis`) name a command that only reads
/// state: help and version output, `status`, `audit`, `snapshot list`,
/// `config show`, and `config validate`. `aegis off` and `aegis rollback`
/// change Aegis itself and do not match.
fn is_read_only_aegis_args(args: &[&str]) -> bool {
    let is_help_flag = |arg: &str| arg == "--help" || arg == "-h";
    match args {
        ["--version" | "-V"] | ["status"] | ["snapshot", "list"] => true,
        ["config", "show" | "validate", ..] | ["audit", ..] | ["help", ..] => true,
        [words @ .., last] if is_help_flag(last) => words.iter().all(|word| !word.starts_with('-')),
        _ => false,
    }
}

/// One simple command: its words with redirections removed, and whether a `|`
/// feeds its stdin.
struct SimpleCommand<'a> {
    words: Vec<&'a str>,
    reads_pipe: bool,
}

/// Split `command` into simple commands. Returns `None` for any character
/// outside the plain-word set, a background `&`, `|&`, input redirection, or a
/// redirection whose target is not `/dev/null` or a descriptor.
fn split_simple_commands(command: &str) -> Option<Vec<SimpleCommand<'_>>> {
    if !command
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || " \t\n-_=.,:/+@|&;>".contains(c))
    {
        return None;
    }

    let mut commands = vec![SimpleCommand {
        words: Vec::new(),
        reads_pipe: false,
    }];
    let bytes = command.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let next = bytes.get(i + 1).copied();
        match bytes[i] {
            b' ' | b'\t' => i += 1,
            b'&' if next == Some(b'&') => {
                commands.push(new_command(false));
                i += 2;
            }
            b'|' if next == Some(b'|') => {
                commands.push(new_command(false));
                i += 2;
            }
            b'|' if next == Some(b'&') => return None,
            b'|' => {
                commands.push(new_command(true));
                i += 1;
            }
            b';' | b'\n' => {
                commands.push(new_command(false));
                i += 1;
            }
            b'&' if next == Some(b'>') => i = skip_redirection(command, i + 1)?,
            b'&' => return None,
            b'>' => i = skip_redirection(command, i)?,
            _ => {
                let start = i;
                while i < bytes.len() && !b" \t\n|&;>".contains(&bytes[i]) {
                    i += 1;
                }
                let word = &command[start..i];
                // A lone descriptor number directly before `>` belongs to the
                // redirection (`2>/dev/null`), not to the command's words.
                let is_descriptor = word.len() == 1 && word.as_bytes()[0].is_ascii_digit();
                if bytes.get(i) == Some(&b'>') && is_descriptor {
                    continue;
                }
                commands.last_mut()?.words.push(word);
            }
        }
    }
    Some(commands)
}

fn new_command(reads_pipe: bool) -> SimpleCommand<'static> {
    SimpleCommand {
        words: Vec::new(),
        reads_pipe,
    }
}

/// Skip one output redirection that starts at the `>` at `start`, and return
/// the index after it. Accepts `>&N`, `>/dev/null`, and `>>/dev/null`, with
/// optional blanks before the target. Returns `None` for any other target.
fn skip_redirection(command: &str, start: usize) -> Option<usize> {
    let bytes = command.as_bytes();
    let mut i = start + 1;
    if bytes.get(i) == Some(&b'&') {
        let descriptor = bytes.get(i + 1)?;
        let after = bytes.get(i + 2);
        let ends = after.is_none_or(|b| b" \t\n|&;".contains(b));
        return (descriptor.is_ascii_digit() && ends).then_some(i + 2);
    }
    if bytes.get(i) == Some(&b'>') {
        i += 1;
    }
    while matches!(bytes.get(i), Some(b' ' | b'\t')) {
        i += 1;
    }
    let target_start = i;
    while i < bytes.len() && !b" \t\n|&;>".contains(&bytes[i]) {
        i += 1;
    }
    (&command[target_start..i] == "/dev/null").then_some(i)
}
