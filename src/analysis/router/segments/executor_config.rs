//! Executor-carrying option values and environment-variable assignments the
//! unclaimed-interpreter net (issue #384/#430) must scan even when the
//! stage's own program sits on [`super::unclaimed`]'s `NAME_ONLY_PROGRAMS`.
//! `git`, `man`, and `less` legitimately name a command as data almost
//! everywhere, but a handful of their own options and environment variables
//! instead *run* whatever value they carry — `git -c core.pager=...`,
//! `LESSOPEN=... less`, `man -P ...` — which would otherwise let an
//! interpreter smuggled through one of them slip past the exclusion list
//! meant for read-only uses of those programs.

use super::unclaimed::route_executor_value;
use super::{HomeState, RouteContext, RoutedTarget, is_assignment_only_stage, is_shell_identifier};
use aegis_types::DegradationReason;

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

/// Every route `raw_tokens`' leading `NAME=value` shell environment
/// assignments — directly, or past a leading `env` launcher and its own
/// flags — produce: each assignment on [`EXECUTOR_ENV_VARS`] a value that
/// names a known interpreter or is otherwise path-like
/// (`LESSOPEN='|python3 ./evil.py %s' less notes.txt`, `env PAGER="python3
/// ./evil.py" man ls`, issue #384/#430, Rule B) contributes its own entry.
/// Stops at the first token that is not itself an assignment — that token
/// is the program, and nothing at or past it is a leading assignment any
/// more, so a same-shaped positional operand after the program (`man ls
/// PAGER=cat`) is left alone. Collects every distinct `LauncherOperand`
/// (`EDITOR=/dev/null MANPAGER=./b man ls` candidates on both) and returns
/// early with a single-element `Unresolved` the moment any value names a
/// known interpreter — nothing past that can un-degrade the stage.
pub(super) fn env_prefix_executor_route(
    raw_tokens: &[&str],
    home: HomeState,
    ctx: &RouteContext<'_>,
) -> Vec<RoutedTarget> {
    let mut routes = Vec::new();
    for token in env_launcher_tail(raw_tokens) {
        let Some((name, value)) = token.split_once('=') else {
            break;
        };
        if !is_shell_identifier(name) {
            break;
        }
        if EXECUTOR_ENV_VARS.contains(&name)
            && let Some(route) = route_executor_value(value, home, ctx)
        {
            if matches!(route, RoutedTarget::Unresolved { .. }) {
                return vec![route];
            }
            routes.push(route);
        }
    }
    routes
}

/// `raw_tokens` itself when its first token is not the `env` launcher, or
/// the tokens past `env` and its own leading flags (`-i`/`--ignore-
/// environment`, `-0`/`--null`, `-u`/`--unset NAME`, `-C`/`--chdir DIR`)
/// otherwise — so an assignment `env` carries (`env PAGER="python3
/// ./evil.py" man ls`, `env -i PAGER="python3 ./evil.py" man ls`) is scanned
/// exactly like a bare leading assignment already is (issue #384/#430).
/// Does not attempt `env -S`/`--split-string`'s own splitting — that shape
/// resolves to its own effective program elsewhere in this crate, and a
/// value it carries is out of scope here.
fn env_launcher_tail<'a>(raw_tokens: &'a [&'a str]) -> &'a [&'a str] {
    let Some((&first, rest)) = raw_tokens.split_first() else {
        return raw_tokens;
    };
    if first.rsplit('/').next() != Some("env") {
        return raw_tokens;
    }
    let mut index = 0;
    while index < rest.len() {
        let token = rest[index];
        if matches!(token, "-i" | "-0" | "--ignore-environment" | "--null") {
            index += 1;
        } else if matches!(token, "-u" | "--unset" | "-C" | "--chdir") {
            index += 2.min(rest.len() - index);
        } else if token.starts_with("--chdir=") || token.starts_with("--unset=") {
            index += 1;
        } else {
            break;
        }
    }
    &rest[index..]
}

/// Bash's own "declare this as exported" keywords that can precede a plain
/// `NAME=value` assignment stage without changing what the assignment means
/// for an executor environment variable's value (issue #384/#430):
/// `export`, `readonly`, `declare`, `typeset`, and `local`, with any flags
/// that follow them (`-x`, `-gx`, `-x -g`). A flag set that does not export
/// the name still skips here: a later `export NAME` on the same line would,
/// and routing does not track that, so the fail-closed call is to scan it.
pub(super) fn skip_assignment_keyword<'a>(tokens: &'a [&'a str]) -> &'a [&'a str] {
    match tokens {
        [keyword, rest @ ..]
            if matches!(
                *keyword,
                "export" | "readonly" | "declare" | "typeset" | "local"
            ) =>
        {
            let flags = rest.iter().take_while(|tok| tok.starts_with('-')).count();
            &rest[flags..]
        }
        _ => tokens,
    }
}

/// `raw_tokens`' own route(s) when it is nothing but a variable-assignment
/// stage — bare `NAME=value` (possibly with a redirection glued in), or one
/// introduced by `export`/`declare -x`/`typeset -x`/`readonly` — naming an
/// [`EXECUTOR_ENV_VARS`] variable a value that names a known interpreter (a
/// degrading `Unresolved`) or is otherwise path-like (a `LauncherOperand`,
/// Rule B, issue #384/#430): `export PAGER=./pyx; man ls` now candidates on
/// `./pyx` the same way `PAGER=./pyx man ls` already does through
/// [`route_executor_value`] — the value never reaches that check on its own
/// otherwise, because a pure-assignment stage invokes no program for the D4
/// operand walk in [`super::unclaimed::unclaimed_interpreter_net`] to scan.
/// Degrades the assignment stage itself rather than waiting to see whether a
/// later, separately routed stage on the same line reads that variable
/// (`export PAGER="python3 ./evil.py"; man ls`, `PAGER="python3 ./evil.py";
/// man ls`, issue #384/#430) — routing does not track a variable's value
/// across stage boundaries to confirm one will, so the fail-closed call is
/// to treat every such assignment as if it will be. Uses the shared
/// [`super::is_assignment_only_stage`] check (Rule A) for the "is this stage
/// nothing but assignments" shape, and [`route_executor_value`] for the
/// interpreter-name/path-like check itself, so an assignment stage reuses
/// the exact same decorator strip and shebang-check treatment every other
/// executor value gets rather than a second copy of that logic. Collects
/// every distinct `LauncherOperand` several assignments on the same stage
/// produce (`export EDITOR=./a VISUAL=./b`) and returns early with a
/// single-element `Unresolved` the moment any value names a known
/// interpreter.
pub(super) fn assignment_stage_executor_routes(
    raw_tokens: &[&str],
    home: HomeState,
    ctx: &RouteContext<'_>,
) -> Vec<RoutedTarget> {
    let tokens = skip_assignment_keyword(raw_tokens);
    if !is_assignment_only_stage(tokens) {
        return Vec::new();
    }
    let mut routes = Vec::new();
    for token in tokens {
        let Some((name, value)) = token.split_once('=') else {
            continue;
        };
        if !is_shell_identifier(name) || !EXECUTOR_ENV_VARS.contains(&name) {
            continue;
        }
        let Some(route) = route_executor_value(value, home, ctx) else {
            continue;
        };
        if matches!(route, RoutedTarget::Unresolved { .. }) {
            return vec![route];
        }
        routes.push(route);
    }
    routes
}

/// Dispatches to a program-specific option whose value names a known
/// interpreter (issue #384/#430): git's `-c`/`--config-env`, or man's
/// `-P`/`--pager=`. `program` picks which syntax applies — the two share the
/// "option value names a program" shape but nothing else. `raw_tokens` — the
/// stage's own tokens before launcher/assignment stripping — is threaded
/// through to git's `--config-env` handling, which resolves its value
/// against a leading assignment there rather than the stripped `tokens`
/// (review comment 4091038640); man's `-P`/`--pager` carries no such
/// indirection and ignores it.
///
/// Besides its routes, returns every raw token whose *entire* text was
/// already read as an executor value's separate-token form (`-o VALUE`,
/// `-c KEY=VALUE`, `-P VALUE`, ...): that token does not itself start with
/// `-`, so it would otherwise still look like an ordinary, unparsed operand
/// to [`super::unclaimed::unclaimed_interpreter_net`]'s own generic
/// candidate walk once this scan stops returning immediately (issue
/// #384/#430) — `ssh -o 'ProxyCommand ./evil.py %h' host` must candidate on
/// `./evil.py` exactly once, not once through this scan's own
/// leading-word extraction and once more as the literal (space-containing,
/// nonsensical-as-a-path) whole option value. An `=`-glued form
/// (`-oKEY=VALUE`, `--pager=VALUE`) needs no such entry: the generic walk's
/// own `d4_value` split reads the same value out of that whole token a
/// second time, but lands on the identical candidate this scan already
/// routed, so `segments.rs`'s own `push_unique` collapses the two into one
/// target when it merges this stage's routes. A no-`=` glued form
/// (`-Pvalue`) is different: nothing here marks where its flag letters end
/// and its value begins, so program-specific dispatch — like this
/// function's own `-P` handling below — still owns identifying it; the
/// generic walk's short-flag-letter-run guessing (GHSA-xj54 follow-up)
/// exists for programs with no dispatch entry here at all, and can surface
/// an extra, differently-split candidate alongside the real one rather than
/// landing on the identical string `push_unique` would collapse.
pub(super) fn option_value_executor_route<'a>(
    program: &str,
    raw_tokens: &[&str],
    tokens: &[&'a str],
    home: HomeState,
    ctx: &RouteContext<'_>,
) -> (Vec<RoutedTarget>, Vec<&'a str>) {
    match program {
        "git" => git_config_executor_route(raw_tokens, tokens, home, ctx),
        "man" => man_pager_executor_route(tokens, home, ctx),
        "ssh" | "scp" | "sftp" => ssh_option_executor_route(tokens, home, ctx),
        "rg" => long_option_executor_route("--pre", tokens, home, ctx),
        "ag" => long_option_executor_route("--pager", tokens, home, ctx),
        _ => (Vec::new(), Vec::new()),
    }
}

/// Scans SSH-family `-o` options whose values name a command OpenSSH later
/// runs: `ProxyCommand`, `LocalCommand`, and `KnownHostsCommand`. OpenSSH
/// accepts both a separate `-o VALUE` and glued `-oVALUE`, also at the end of
/// a short-flag group (`-vo VALUE`, `-voVALUE`); each value is a
/// `Key value` or `Key=value` pair. An `o` that is really the value of an
/// earlier flag in the group (`-io`) is scanned too, and the next token is
/// only peeked, never skipped, so a real `-o` after it is still read.
/// Collects every distinct `LauncherOperand` several `-o` occurrences
/// produce (`ssh -o ProxyCommand=./a -o LocalCommand='python3 ./b' host`
/// candidates on `./a`, then degrades the stage on `./b`'s interpreter
/// name), returning early with a single-element `Unresolved` the moment any
/// value names a known interpreter.
fn ssh_option_executor_route<'a>(
    tokens: &[&'a str],
    home: HomeState,
    ctx: &RouteContext<'_>,
) -> (Vec<RoutedTarget>, Vec<&'a str>) {
    let mut routes = Vec::new();
    let mut consumed = Vec::new();
    for (idx, token) in tokens.iter().enumerate() {
        let Some(group) = token.strip_prefix('-').filter(|g| !g.starts_with('-')) else {
            continue;
        };
        let Some((_, glued)) = group.split_once('o') else {
            continue;
        };
        let separate_token = glued.is_empty();
        let option = if separate_token {
            tokens.get(idx + 1).copied()
        } else {
            Some(glued)
        };
        let Some((key, value)) = option.and_then(ssh_option_key_value) else {
            continue;
        };
        if is_ssh_executor_option_key(key)
            && let Some(route) = route_executor_value(value, home, ctx)
        {
            if separate_token && let Some(&next) = tokens.get(idx + 1) {
                consumed.push(next);
            }
            if matches!(route, RoutedTarget::Unresolved { .. }) {
                return (vec![route], consumed);
            }
            routes.push(route);
        }
    }
    (routes, consumed)
}

/// `true` for an OpenSSH option whose value is executed as a command.
fn is_ssh_executor_option_key(key: &str) -> bool {
    ["ProxyCommand", "LocalCommand", "KnownHostsCommand"]
        .into_iter()
        .any(|executor_key| key.eq_ignore_ascii_case(executor_key))
}

/// Splits an OpenSSH `-o` argument into its key and command value. Like
/// OpenSSH, the key ends at the first whitespace or `=`, and one `=` with
/// optional whitespace around it may separate the key from the value, so a
/// `=` inside the command (`ProxyCommand python3 -c x=1`) stays in the value.
fn ssh_option_key_value(option: &str) -> Option<(&str, &str)> {
    let option = option.trim_start();
    let key_end = option.find(|c: char| c == '=' || c.is_whitespace())?;
    let (key, rest) = option.split_at(key_end);
    let rest = rest.trim_start();
    let value = rest.strip_prefix('=').unwrap_or(rest).trim_start();
    Some((key, value))
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
/// `--config-env` besides. `-c`'s right-hand side is the config value
/// itself, but `--config-env`'s right-hand side names an *environment
/// variable* git reads the value from — `git --config-env=core.pager=RUNNER`
/// runs whatever `$RUNNER` holds, not a literal program named `RUNNER`
/// (locally inspected `git --help`, review comment 4091038640). Resolved
/// against `raw_tokens`' own leading assignment prefix (`RUNNER='python3
/// ./evil.py' git --config-env=core.pager=RUNNER -p log`); an executor key
/// naming a variable that assignment prefix does not set is exactly as
/// opaque as one this file cannot resolve at all — the value could be
/// anything in the inherited environment — so it fails closed rather than
/// passing the unresolved name through [`route_executor_value`], which would
/// read `RUNNER` as a literal (and non-matching) program name. Collects every
/// distinct `LauncherOperand` several `-c`/`--config-env` occurrences on the
/// same invocation produce, and returns early with a single-element
/// `Unresolved` the moment any value names a known interpreter. Also returns
/// every separate-token `<key>=<value>` argument a route was found for, so
/// the caller can keep the generic operand walk from reading the same
/// argument a second time as an unparsed literal (issue #384/#430).
fn git_config_executor_route<'a>(
    raw_tokens: &[&str],
    tokens: &[&'a str],
    home: HomeState,
    ctx: &RouteContext<'_>,
) -> (Vec<RoutedTarget>, Vec<&'a str>) {
    let mut routes = Vec::new();
    let mut consumed = Vec::new();
    let mut iter = tokens.iter();
    while let Some(&tok) = iter.next() {
        let (assignment, names_an_env_var, separate_token) = if tok == "-c" {
            (iter.next().copied(), false, true)
        } else if tok == "--config-env" {
            (iter.next().copied(), true, true)
        } else if let Some(rest) = tok.strip_prefix("-c") {
            (Some(rest), false, false)
        } else {
            (tok.strip_prefix("--config-env="), true, false)
        };
        let Some((key, value)) = assignment.and_then(|a| a.split_once('=')) else {
            continue;
        };
        if !is_executor_config_key(key) {
            continue;
        }
        let route = if names_an_env_var {
            match assignment_prefix_value(raw_tokens, value) {
                Some(resolved) => route_executor_value(resolved, home, ctx),
                None => Some(RoutedTarget::Unresolved {
                    reason: DegradationReason::DynamicSource,
                }),
            }
        } else {
            route_executor_value(value, home, ctx)
        };
        if let Some(route) = route {
            if separate_token && let Some(a) = assignment {
                consumed.push(a);
            }
            if matches!(route, RoutedTarget::Unresolved { .. }) {
                return (vec![route], consumed);
            }
            routes.push(route);
        }
    }
    (routes, consumed)
}

/// The value a leading `NAME=value` shell-environment assignment in
/// `raw_tokens` gives `name` — directly, or past a leading `env` launcher
/// and its own flags, the same reach [`env_prefix_names_an_interpreter`]
/// uses — or `None` when `raw_tokens` assigns no such variable (issue
/// #384/#430, review comment 4091038640).
fn assignment_prefix_value<'a>(raw_tokens: &'a [&'a str], name: &str) -> Option<&'a str> {
    for token in env_launcher_tail(raw_tokens) {
        let Some((defined, value)) = token.split_once('=') else {
            break;
        };
        if !is_shell_identifier(defined) {
            break;
        }
        if defined == name {
            return Some(value);
        }
    }
    None
}

/// Scans for `-P <value>`, `--pager <value>`, glued `--pager=<value>`, and
/// glued `-P<value>` (`man -P./pyx ls`, GHSA-xj54 follow-up: `man` sits on
/// [`super::unclaimed::NAME_ONLY_PROGRAMS`] and returns before the generic
/// walk's own short-flag-letter-run guessing ever sees its tokens, so a
/// no-`=` glued `-P` value needs its own read here instead). Collects every
/// distinct `LauncherOperand` several occurrences produce, returning early
/// with a single-element `Unresolved` the moment any value names a known
/// interpreter. Also returns every separate-token `<value>` argument a
/// route was found for, so the caller can keep the generic operand walk
/// from reading the same argument a second time (issue #384/#430).
fn man_pager_executor_route<'a>(
    tokens: &[&'a str],
    home: HomeState,
    ctx: &RouteContext<'_>,
) -> (Vec<RoutedTarget>, Vec<&'a str>) {
    let mut routes = Vec::new();
    let mut consumed = Vec::new();
    let mut iter = tokens.iter();
    while let Some(&tok) = iter.next() {
        let separate_token = tok == "-P" || tok == "--pager";
        let value = if separate_token {
            iter.next().copied()
        } else {
            tok.strip_prefix("--pager=")
                .or_else(|| tok.strip_prefix("-P").filter(|v| !v.is_empty()))
        };
        if let Some(route) = value.and_then(|value| route_executor_value(value, home, ctx)) {
            if separate_token && let Some(v) = value {
                consumed.push(v);
            }
            if matches!(route, RoutedTarget::Unresolved { .. }) {
                return (vec![route], consumed);
            }
            routes.push(route);
        }
    }
    (routes, consumed)
}

/// Scans for a separate-token `<flag> <value>` or glued `<flag>=<value>` —
/// `rg`'s `--pre` (its preprocessor command) and `ag`'s `--pager`, read the
/// same way `man -P`/`--pager` is (GHSA-xj54 follow-up): both `rg` and
/// `ag` sit on [`super::unclaimed::NAME_ONLY_PROGRAMS`], so a command
/// smuggled through either option would otherwise stay invisible to
/// routing. No short-flag or no-`=` glued form exists for either option, so
/// unlike [`man_pager_executor_route`] this has no such case to read.
/// Matches `flag` exactly against the whole token, so a same-prefixed
/// sibling option (`--pre-glob`) is never mistaken for it. Stops at the
/// first bare `--`: both `rg` and `ag` end their own option parsing there,
/// so `flag`-shaped text past it is a positional pattern or path argument,
/// not the option (`rg -- --pre ./pyx foo .` must not route `./pyx`, GHSA-
/// xj54 follow-up). Collects every distinct `LauncherOperand` several
/// occurrences produce, returning early with a single-element `Unresolved`
/// the moment any value names a known interpreter, and returns every
/// separate-token value argument a route was found for so the caller can
/// keep the generic operand walk from reading it a second time (issue
/// #384/#430).
fn long_option_executor_route<'a>(
    flag: &str,
    tokens: &[&'a str],
    home: HomeState,
    ctx: &RouteContext<'_>,
) -> (Vec<RoutedTarget>, Vec<&'a str>) {
    let glued_prefix = format!("{flag}=");
    let mut routes = Vec::new();
    let mut consumed = Vec::new();
    let mut iter = tokens.iter();
    while let Some(&tok) = iter.next() {
        if tok == "--" {
            break;
        }
        let separate_token = tok == flag;
        let value = if separate_token {
            iter.next().copied()
        } else {
            tok.strip_prefix(glued_prefix.as_str())
        };
        if let Some(route) = value.and_then(|value| route_executor_value(value, home, ctx)) {
            if separate_token && let Some(v) = value {
                consumed.push(v);
            }
            if matches!(route, RoutedTarget::Unresolved { .. }) {
                return (vec![route], consumed);
            }
            routes.push(route);
        }
    }
    (routes, consumed)
}
