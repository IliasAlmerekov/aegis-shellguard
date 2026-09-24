//! Fail-closed net for a stage no other routing path claims (issue #384,
//! issue #430, ADR-022 §6 amendment).
//!
//! `launcher_prefix_lengths` (`crates/aegis-parser/src/lib.rs`) enumerates
//! launcher words by name — `sudo`, `env`, `timeout`, … — so an unlisted
//! wrapper (`setsid`, `strace`, `ionice`, `taskset`, `find … -exec`, …)
//! leaves its program token unrecognized and the interpreter it carries
//! invisible to every other routing path, and patching that list one word
//! at a time keeps leaking the next one (issue #384/#430). Once a
//! stage reaches the end of routing with no target at all, a later token
//! naming a known interpreter is reason enough to stop trusting the
//! auto-approve path, even without knowing which wrapper carried it.

use super::*;

/// Programs that *name* a command as data — an argument, a search pattern,
/// a documentation topic, a filesystem path to create/remove/rename —
/// without ever running it. A stage whose own (effective, launcher-stripped)
/// program is one of these stays unclaimed exactly as it did before this net
/// existed, even when a later token happens to spell an interpreter name
/// (`echo python3`, `grep -r node src`, `apt install python3`, `git log
/// --grep python3`, `mkdir python3`, `rm -f node`).
const NAME_ONLY_PROGRAMS: &[&str] = &[
    "echo",
    "printf",
    "which",
    "type",
    "whereis",
    "man",
    "info",
    "help",
    "apropos",
    "grep",
    "egrep",
    "fgrep",
    "rg",
    "ag",
    "ls",
    "cat",
    "head",
    "tail",
    "less",
    "more",
    "file",
    "stat",
    "wc",
    "diff",
    "apt",
    "apt-get",
    "apt-cache",
    "dnf",
    "yum",
    "brew",
    "pacman",
    "git",
    "update-alternatives",
    "dpkg",
    "mkdir",
    "rmdir",
    "touch",
    "rm",
    "cp",
    "mv",
    "ln",
    "chmod",
    "chown",
    "chgrp",
    "basename",
    "dirname",
    "realpath",
    "readlink",
];

/// `true` for `command -v ...` / `command -V ...` — POSIX's "print where
/// NAME resolves, do not run it" form. `launcher_prefix_lengths` strips the
/// `command` word unconditionally (it exists to make `command git` route as
/// plain `git`), so by the time [`unclaimed_interpreter_net`] sees the
/// effective program, `command` and the `-v`/`-V` distinction it carried are
/// both already gone; this checks the stage's own raw tokens instead, before
/// that stripping happens.
fn is_command_lookup(raw_tokens: &[&str]) -> bool {
    let Some(first) = raw_tokens.first() else {
        return false;
    };
    basename(first) == "command" && matches!(raw_tokens.get(1), Some(&"-v") | Some(&"-V"))
}

/// The basename of a token — unquoting already happened in
/// [`aegis_parser::split_tokens`], so only a leading path needs stripping
/// (`/usr/bin/python3` → `python3`).
fn basename(token: &str) -> &str {
    token.rsplit('/').next().unwrap_or(token)
}

/// Degrade `stage_raw` when routing has already tried every other path and
/// claimed nothing, yet a token past its own effective program names a
/// known registry interpreter (issue #384/#430): an unenumerated wrapper
/// word (`setsid`, `strace`, `ionice`, `taskset`, …), or a later positional
/// argument a launcher form never inspects (`find … -exec python3 {} \;`).
///
/// Returns an empty `Vec` when the stage's own effective program is on
/// [`NAME_ONLY_PROGRAMS`] or is a `command -v`/`-V` lookup — those name a
/// command without running it, so a mention of an interpreter there is data,
/// not a hidden wrapper — or when no later token resolves to a registry
/// interpreter at all. Reuses [`super::resolve_interpreter`]'s existing
/// exact-match, versioned-basename, and trusted-alias normalization rather
/// than a second copy of that list.
///
/// Callers check this only for a stage that produced no target from any
/// other routing path (`segments.rs`'s two call sites both check first) — a
/// stage that already routed something is left alone.
pub(super) fn unclaimed_interpreter_net(
    stage_raw: &str,
    full_command: &str,
    trusted_aliases: &[(&str, &str)],
) -> Vec<RoutedTarget> {
    // A trailing redirect (`{ echo ok; } 2>/dev/null`) is punctuation, not an
    // argument — stripped before tokenizing so its target never reads as a
    // path-like operand below (issue #384/#430), the same stripping
    // `wrapper_bodies` already applies before its own extraction.
    let owned_tokens = aegis_parser::split_tokens(strip_trailing_redirection(stage_raw));
    let raw_tokens: Vec<&str> = owned_tokens.iter().map(String::as_str).collect();
    if is_command_lookup(&raw_tokens) {
        return Vec::new();
    }

    // A stage that is nothing but a variable assignment (bare `NAME=value`,
    // or one introduced by `export`/`declare -x`/`typeset -x`/`readonly`)
    // degrades on its own when it names an executor environment variable an
    // interpreter, regardless of what a later, separately routed stage on
    // the same line does with it — this stage alone has no target to
    // resolve `slice` against below (issue #384/#430).
    if assignment_stage_names_an_interpreter(&raw_tokens, trusted_aliases) {
        return vec![RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        }];
    }

    let Some(slice) = aegis_parser::effective_token_slices(&raw_tokens)
        .into_iter()
        .next()
    else {
        return Vec::new();
    };

    let operands = slice.tokens[1..].iter().filter(|tok| !tok.starts_with('-'));
    // A program word reached only through expansion the router does not
    // perform — `$VAR`, `${X:-python3}`, `` `cmd` `` — is exactly as opaque
    // as an unenumerated wrapper word: routing has no way to know what
    // actually runs (issue #384/#430). Gated on having an operand at all so
    // a bare `$EDITOR`/`$SHELL` with nothing to act on — an everyday
    // interactive-launch shape — stays exactly as auto-approved as it was
    // before this check existed.
    if operands.clone().next().is_some() {
        if is_dynamic_program_word(slice.program) {
            return vec![RoutedTarget::Unresolved {
                reason: DegradationReason::DynamicSource,
            }];
        }
        // A program word naming a shell `alias` defined earlier resolves
        // only as opaquely as its own replacement text does (issue
        // #384/#430 round 6): standing in for a known interpreter (`alias
        // runpy=python3`), for a dynamic word (`alias n="$X"`), or for
        // nothing parseable at all is exactly as unreadable as an
        // unenumerated wrapper word. An alias for anything else (`alias
        // ll='ls -l'`, `alias g=git`) is not the shape this net exists to
        // catch, so it is left to the rest of routing below — the same
        // `NAME_ONLY_PROGRAMS` check, interpreter-naming-operand scan, and
        // path-like-operand candidate walk an ordinary program word gets.
        if let Some(value) = alias_value(full_command, stage_raw, slice.program) {
            let replacement = value.trim();
            if replacement.is_empty()
                || is_dynamic_program_word(replacement)
                || token_names_an_interpreter(replacement, trusted_aliases)
            {
                return vec![RoutedTarget::Unresolved {
                    reason: DegradationReason::DynamicSource,
                }];
            }
        }
    }

    // A handful of `NAME_ONLY_PROGRAMS` members have their own escape hatch:
    // an option value or environment-variable assignment that *runs* a
    // command instead of naming one as data (`git -c core.pager=...`,
    // `LESSOPEN=... less`, `man -P ...`, issue #384/#430). Checked ahead of
    // the exclusion list below so it applies even to a program that list
    // would otherwise wave through untouched.
    if env_prefix_names_an_interpreter(&raw_tokens, trusted_aliases)
        || option_value_names_an_interpreter(slice.program, &slice.tokens, trusted_aliases)
    {
        return vec![RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        }];
    }

    if NAME_ONLY_PROGRAMS.contains(&slice.program) {
        return Vec::new();
    }

    let names_an_interpreter = slice.tokens[1..]
        .iter()
        .any(|tok| token_names_an_interpreter(tok, trusted_aliases));
    if names_an_interpreter {
        return vec![RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        }];
    }

    // Nothing named a known interpreter, but an unenumerated wrapper
    // (`setsid ./pyx`) may still hand a script its own path-like operand
    // straight through: `resolve` reads the file and only treats it as a
    // target with a verified shebang, so a non-script operand stays safe
    // (issue #384/#430). Routed as `LauncherOperand`, not `DirectExec`,
    // because each candidate here is something the net itself picked out of
    // an unclaimed stage's arguments — not a program the command named — so
    // a missing path or a directory (an everyday shape for an ordinary
    // command's argument) resolves speculatively instead of degrading like
    // a user-typed `DirectExec` still does.
    //
    // A flag's own argument (`setsid -u user ./pyx`) is not filtered out
    // here: the `-`-prefix filter above only removes the flag token itself,
    // not the value that follows it, so a path-like flag value is exactly
    // as much a candidate as the real target is. Routing cannot tell them
    // apart, so every distinct path-like operand becomes its own candidate
    // (issue #384/#430 round 6) — stopping at the first one, as this used
    // to, let a path-like flag value (`setsid -u ./notes.txt ./pyx`) shadow
    // the real script that followed it, resolving to nothing and leaving
    // the actual target unexamined. `resolve`/`resolve_for_analysis` still
    // decide per candidate whether it is real (verified shebang) or nothing
    // (missing, directory, no shebang), so a benign command with several
    // path-like operands (`cp ./a.txt ./b.txt`, `tar -cf ./out.tar
    // ./srcdir`) stays exactly as auto-approved as a single candidate was.
    let mut seen = std::collections::HashSet::new();
    operands
        .filter(|tok| tok.contains('/') && !tok.contains("://") && is_literal_path(tok))
        .filter(|tok| seen.insert(**tok))
        .map(|tok| RoutedTarget::LauncherOperand {
            path: PathBuf::from(*tok),
        })
        .collect()
}

/// `true` when `tok` itself names a known registry interpreter, or — for a
/// token that survived unquoting with embedded whitespace still in it (a
/// quoted multi-word argument, e.g. `"python3 ./evil.py"` passed to `script
/// -c`) — its first word does. `basename` alone mishandles the quoted case:
/// run on the whole token it finds the last `/`-separated segment of the
/// *last* word instead of the interpreter name that opens it (issue
/// #384/#430).
pub(super) fn token_names_an_interpreter(tok: &str, trusted_aliases: &[(&str, &str)]) -> bool {
    if resolve_interpreter(basename(tok), trusted_aliases).is_some() {
        return true;
    }
    tok.contains(char::is_whitespace)
        && tok
            .split_whitespace()
            .next()
            .is_some_and(|word| resolve_interpreter(basename(word), trusted_aliases).is_some())
}
