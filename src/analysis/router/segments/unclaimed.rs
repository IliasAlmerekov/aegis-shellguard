//! Fail-closed net for a stage no other routing path claims (issue #384,
//! issue #430, ADR-022 §6 amendment).
//!
//! `launcher_prefix_lengths` (`crates/aegis-parser/src/lib.rs`) enumerates
//! launcher words by name — `sudo`, `env`, `timeout`, … — so an unlisted
//! wrapper (`setsid`, `strace`, `ionice`, `taskset`, `find … -exec`, …)
//! leaves its program token unrecognized and the interpreter it carries
//! invisible to every other routing path, and patching that list one word
//! at a time keeps leaking the next one (issue #384/#430 round 5). Once a
//! stage reaches the end of routing with no target at all, a later token
//! naming a known interpreter is reason enough to stop trusting the
//! auto-approve path, even without knowing which wrapper carried it.

use super::*;

/// Programs that *name* a command as data — an argument, a search pattern,
/// a documentation topic — without ever running it. A stage whose own
/// (effective, launcher-stripped) program is one of these stays unclaimed
/// exactly as it did before this net existed, even when a later token
/// happens to spell an interpreter name (`echo python3`, `grep -r node
/// src`, `apt install python3`, `git log --grep python3`).
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
/// `None` when the stage's own effective program is on
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
    trusted_aliases: &[(&str, &str)],
) -> Option<RoutedTarget> {
    let owned_tokens = aegis_parser::split_tokens(stage_raw);
    let raw_tokens: Vec<&str> = owned_tokens.iter().map(String::as_str).collect();
    if is_command_lookup(&raw_tokens) {
        return None;
    }

    let slice = aegis_parser::effective_token_slices(&raw_tokens)
        .into_iter()
        .next()?;
    if NAME_ONLY_PROGRAMS.contains(&slice.program) {
        return None;
    }

    let names_an_interpreter = slice.tokens[1..]
        .iter()
        .any(|tok| resolve_interpreter(basename(tok), trusted_aliases).is_some());

    names_an_interpreter.then_some(RoutedTarget::Unresolved {
        reason: DegradationReason::DynamicSource,
    })
}
