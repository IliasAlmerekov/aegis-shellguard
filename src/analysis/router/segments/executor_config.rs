//! Executor-carrying option values and environment-variable assignments the
//! unclaimed-interpreter net (issue #384/#430) must scan even when the
//! stage's own program sits on [`super::unclaimed`]'s `NAME_ONLY_PROGRAMS`.
//! `git`, `man`, and `less` legitimately name a command as data almost
//! everywhere, but a handful of their own options and environment variables
//! instead *run* whatever value they carry — `git -c core.pager=...`,
//! `LESSOPEN=... less`, `man -P ...` — which would otherwise let an
//! interpreter smuggled through one of them slip past the exclusion list
//! meant for read-only uses of those programs.

use super::unclaimed::token_names_an_interpreter;

/// Environment variables whose assigned value names a command the shell
/// later runs, not a value it reads (issue #384/#430).
const EXECUTOR_ENV_VARS: &[&str] = &[
    "PAGER",
    "GIT_PAGER",
    "MANPAGER",
    "EDITOR",
    "VISUAL",
    "GIT_EDITOR",
    "GIT_SSH_COMMAND",
    "GIT_SSH",
    "GIT_ASKPASS",
    "SSH_ASKPASS",
    "GIT_EXTERNAL_DIFF",
    "GIT_SEQUENCE_EDITOR",
    "GIT_PROXY_COMMAND",
    "BROWSER",
    "LESSOPEN",
    "LESSCLOSE",
];

/// `true` when `raw_tokens` opens with one or more `NAME=value` shell
/// environment assignments and at least one assigns a name on
/// [`EXECUTOR_ENV_VARS`] a value that names a known interpreter
/// (`LESSOPEN='|python3 ./evil.py %s' less notes.txt`, issue #384/#430).
/// Stops at the first token that is not itself an assignment — that token is
/// the program, and nothing at or past it is a leading assignment any more,
/// so a same-shaped positional operand after the program (`man ls
/// PAGER=cat`) is left alone.
pub(super) fn env_prefix_names_an_interpreter(
    raw_tokens: &[&str],
    trusted_aliases: &[(&str, &str)],
) -> bool {
    for token in raw_tokens {
        let Some((name, value)) = token.split_once('=') else {
            break;
        };
        if !is_shell_identifier(name) {
            break;
        }
        if EXECUTOR_ENV_VARS.contains(&name) && value_names_an_interpreter(value, trusted_aliases) {
            return true;
        }
    }
    false
}

/// `true` when `name` is a valid POSIX shell identifier: a leading letter or
/// underscore, then only alphanumerics or underscores.
fn is_shell_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `true` when a token in `tokens` (a stage's effective tokens, program at
/// index 0) is a program-specific option whose value names a known
/// interpreter (issue #384/#430): git's `-c`/`--config-env`, or man's
/// `-P`/`--pager=`. `program` picks which syntax applies — the two share the
/// "option value names a program" shape but nothing else.
pub(super) fn option_value_names_an_interpreter(
    program: &str,
    tokens: &[&str],
    trusted_aliases: &[(&str, &str)],
) -> bool {
    match program {
        "git" => git_config_value_names_an_interpreter(tokens, trusted_aliases),
        "man" => man_pager_value_names_an_interpreter(tokens, trusted_aliases),
        _ => false,
    }
}

/// git config keys whose value names a command git later runs rather than
/// plain configuration data. Matched per key family rather than as one flat
/// list, since git lets the caller pick several of these families' own
/// middle segment: `alias.*` covers every alias name (`alias.x=...`),
/// `credential.*.helper` a URL-scoped credential helper alongside the
/// top-level `credential.helper`, `gpg.*.program` a format-scoped signing
/// program alongside the top-level `gpg.program`, and `filter.*.clean`/
/// `.smudge`/`.process`, `diff.*.command`/`.textconv`, `merge.*.driver` a
/// user-named filter, diff, or merge driver (issue #384/#430).
fn is_executor_config_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    let exact = matches!(
        lower.as_str(),
        "core.pager"
            | "core.editor"
            | "core.sshcommand"
            | "core.askpass"
            | "core.gitproxy"
            | "core.fsmonitor"
            | "diff.external"
            | "credential.helper"
            | "sequence.editor"
            | "gpg.program"
            | "uploadpack.packobjectshook"
            | "sendemail.sendmailcmd"
    );
    let family = lower.starts_with("alias.")
        || family_middle_segment(&lower, "credential.", ".helper").is_some()
        || family_middle_segment(&lower, "gpg.", ".program").is_some()
        || [
            ("filter.", ".clean"),
            ("filter.", ".smudge"),
            ("filter.", ".process"),
            ("diff.", ".command"),
            ("diff.", ".textconv"),
            ("merge.", ".driver"),
        ]
        .into_iter()
        .any(|(prefix, suffix)| family_middle_segment(&lower, prefix, suffix).is_some());
    exact || family
}

/// The user-chosen middle segment of a `<prefix><name><suffix>` git config
/// key family (`credential.<url>.helper`, `gpg.<format>.program`,
/// `filter.<name>.clean`, ...), or `None` when `key` does not carry this
/// family's own prefix and suffix (issue #384/#430).
fn family_middle_segment<'a>(key: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    key.strip_prefix(prefix)?.strip_suffix(suffix)
}

/// Scans for `-c <key>=<value>`, glued `-c<key>=<value>`, `--config-env
/// <key>=<value>`, and glued `--config-env=<key>=<value>` — git accepts a
/// short option's argument either attached or as the next token, and
/// `--config-env` besides.
fn git_config_value_names_an_interpreter(
    tokens: &[&str],
    trusted_aliases: &[(&str, &str)],
) -> bool {
    let mut iter = tokens.iter();
    while let Some(&tok) = iter.next() {
        let assignment = if tok == "-c" || tok == "--config-env" {
            iter.next().copied()
        } else if let Some(rest) = tok.strip_prefix("-c") {
            Some(rest)
        } else {
            tok.strip_prefix("--config-env=")
        };
        let Some((key, value)) = assignment.and_then(|a| a.split_once('=')) else {
            continue;
        };
        if is_executor_config_key(key) && value_names_an_interpreter(value, trusted_aliases) {
            return true;
        }
    }
    false
}

/// Scans for `-P <value>` and glued `--pager=<value>`.
fn man_pager_value_names_an_interpreter(tokens: &[&str], trusted_aliases: &[(&str, &str)]) -> bool {
    let mut iter = tokens.iter();
    while let Some(&tok) = iter.next() {
        let value = if tok == "-P" {
            iter.next().copied()
        } else {
            tok.strip_prefix("--pager=")
        };
        if value.is_some_and(|value| value_names_an_interpreter(value, trusted_aliases)) {
            return true;
        }
    }
    false
}

/// `true` when `value` — an option's argument or an environment
/// assignment's right-hand side — names a known interpreter, after
/// stripping the single leading decorator byte some of these values carry
/// as their own syntax: `LESSOPEN`'s `|cmd` pipe-filter marker, and a git
/// `alias.NAME` value's leading `!` that marks it as a shell command rather
/// than another git subcommand (issue #384/#430).
fn value_names_an_interpreter(value: &str, trusted_aliases: &[(&str, &str)]) -> bool {
    let stripped = value
        .strip_prefix('|')
        .or_else(|| value.strip_prefix('!'))
        .unwrap_or(value);
    token_names_an_interpreter(stripped, trusted_aliases)
}
