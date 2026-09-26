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
use std::ops::ControlFlow;

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

/// `true` when `text` carries ANSI-C quoting (`$'...'`) outside single and
/// double quotes (Rule C, issue #384/#430). Quote/backslash-aware the same
/// way [`aegis_parser::mask_command_substitutions`]'s own scan is — a
/// backslash outside a single quote escapes the next character, and `'`/`"`
/// toggle quote state — so a quoted or escaped `$'` (`'$('`, `"\$'"`) does
/// not trip this.
fn contains_unquoted_ansi_c_quote(text: &str) -> bool {
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut chars = text.char_indices();
    while let Some((idx, ch)) = chars.next() {
        match ch {
            '\\' if !in_single_quote => {
                chars.next();
            }
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            '$' if !in_single_quote
                && !in_double_quote
                && text[idx + ch.len_utf8()..].starts_with('\'') =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}

/// `true` when `raw_tokens` is nothing but a variable-assignment stage —
/// bare `NAME=value` (possibly several, possibly with a redirection glued
/// in), or one introduced by `export`/`declare`/`typeset`/`readonly`/`local`
/// — with no program following. Uses the shared
/// [`super::is_assignment_only_stage`] check (decision D1, GHSA-xj54, Rule A).
fn is_pure_assignment_stage(raw_tokens: &[&str]) -> bool {
    is_assignment_only_stage(skip_assignment_keyword(raw_tokens))
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
/// stage that already routed something is left alone. `home` is the
/// caller's current [`HomeState`] at this stage's position in the command
/// (decision D1, GHSA-xj54): `Degraded` withholds `ctx.home` from a `~/rest`
/// launcher operand this call would otherwise resolve.
pub(super) fn unclaimed_interpreter_net(
    stage_raw: &str,
    ctx: &RouteContext<'_>,
    home: HomeState,
) -> Vec<RoutedTarget> {
    // A `Data consumer` heredoc body (`jq -c . <<'JSON'`, issue #396) is data
    // at rest, not argv — blanked the same way the scanner blanks it
    // (`aegis_parser::mask_inert_heredoc_substitution_markers`) before this
    // stage's own text is tokenized below, so a JSON string that happens to
    // read like an interpreter invocation is never mistaken for one. A body
    // fed to a non-consumer (`xargs <<'EOF'`) is untouched and still
    // tokenized as before — `xargs` turns its stdin into argv for real.
    // Skips the mask call's own allocation on the overwhelmingly common
    // heredoc-free stage (`CONVENTION.md` §8).
    let masked_stage_raw;
    let stage_raw = if stage_raw.contains("<<") {
        masked_stage_raw = aegis_parser::mask_inert_heredoc_substitution_markers(stage_raw);
        masked_stage_raw.as_str()
    } else {
        stage_raw
    };

    // A trailing redirect (`{ echo ok; } 2>/dev/null`) is punctuation, not an
    // argument — stripped before tokenizing so its target never reads as a
    // path-like operand below (issue #384/#430), the same stripping
    // `wrapper_bodies` already applies before its own extraction.
    let stage_text = strip_trailing_redirection(stage_raw);
    let owned_tokens = aegis_parser::split_tokens(stage_text);
    let raw_tokens: Vec<&str> = owned_tokens.iter().map(String::as_str).collect();
    if is_command_lookup(&raw_tokens) {
        return Vec::new();
    }

    // A stage that is nothing but a variable assignment (bare `NAME=value`,
    // or one introduced by `export`/`declare -x`/`typeset -x`/`readonly`)
    // routes on its own when it names an executor environment variable a
    // value that names a known interpreter (degrading `Unresolved`) or is
    // otherwise path-like (a `LauncherOperand`, Rule B, issue #384/#430),
    // regardless of what a later, separately routed stage on the same line
    // does with it — this stage alone has no target to resolve `slice`
    // against below.
    let mut targets = assignment_stage_executor_routes(&raw_tokens, home, ctx);
    if !targets.is_empty() {
        // Either one or more `LauncherOperand`s, or the single `Unresolved`
        // [`assignment_stage_executor_routes`] short-circuits to itself —
        // both are already this stage's whole answer.
        return targets;
    }

    // A stage that is nothing but a variable assignment invokes no program
    // at all — bash never runs anything for a bare `NAME=value` (or one
    // introduced by `export`/`declare`/`typeset`/`readonly`/`local`) — so
    // its own assignment values are never a launcher-operand candidate just
    // because one happens to contain a `/` (`export PATH=/x`; decision D1,
    // GHSA-xj54). Checked after the interpreter-naming case above, which
    // must still win when an assignment's own value names one.
    if is_pure_assignment_stage(&raw_tokens) {
        return Vec::new();
    }

    // A `for`/`select` loop header's own list (`for x in ./pyx; do "$x";
    // done`) is left to the ordinary operand scan below rather than
    // special-cased away: the loop variable feeds the body the same way an
    // ordinary launcher operand does, so a path-like list entry is exactly
    // as much a candidate as `setsid ./pyx`'s own operand is (issue
    // #384/#430, GHSA-xj54). `stage_degrades_home_trust`
    // (`segments.rs`) already withholds `HomeState` trust for this shape on
    // its own, ahead of this call, so nothing here needs to repeat that.

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
        // #384/#430): standing in for a known interpreter anywhere in its
        // words (`alias runpy=python3`, `alias n='X=1 python3'`), for a dynamic word (`alias n="$X"`), or for
        // nothing parseable at all is exactly as unreadable as an
        // unenumerated wrapper word. An alias for anything else (`alias
        // ll='ls -l'`, `alias g=git`) is not the shape this net exists to
        // catch, so it is left to the rest of routing below — the same
        // `NAME_ONLY_PROGRAMS` check, interpreter-naming-operand scan, and
        // path-like-operand candidate walk an ordinary program word gets.
        let alias_scope = ctx
            .command
            .get(..ctx.alias_scope_end.get())
            .unwrap_or(ctx.command);
        if let Some(value) = alias_value(alias_scope, slice.program) {
            let replacement = value.trim();
            if replacement.is_empty()
                || is_dynamic_program_word(replacement)
                || replacement
                    .split_whitespace()
                    .any(|word| token_names_an_interpreter(word, ctx.trusted_aliases))
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
    // would otherwise wave through untouched. Every distinct executor value
    // on the stage gets its own candidate (`EDITOR=/dev/null setsid ./pyx`,
    // `ssh -o ProxyCommand=./a -o LocalCommand='python3 ./b' host`) — the
    // first one found used to return immediately and hide the rest of the
    // stage from every check below it, including the stage's own operand
    // walk (issue #384/#430). Any single `Unresolved` still degrades the
    // whole stage at once, since nothing past it can un-degrade it.
    let (option_routes, option_consumed) =
        option_value_executor_route(slice.program, &raw_tokens, &slice.tokens, home, ctx);
    for route in env_prefix_executor_route(&raw_tokens, home, ctx)
        .into_iter()
        .chain(option_routes)
    {
        if matches!(route, RoutedTarget::Unresolved { .. }) {
            return vec![route];
        }
        targets.push(route);
    }

    if NAME_ONLY_PROGRAMS.contains(&slice.program) {
        return targets;
    }

    // The tokenizer does not understand ANSI-C quoting (`$'...'`), so it can
    // glue a real operand into the garbage token that results (Rule C, issue
    // #384/#430): `strace -o $'\'$(echo x' ./pyx ')'` tokenizes to nonsense
    // that still happens to contain a `/`, which the path-like walk below
    // would otherwise read as a literal path. Degrading here no longer
    // requires `operands` (computed above) to hold a survivor: the same
    // mis-tokenization can eat the operand entirely rather than just glue
    // garbage onto it (`strace -o$'\'' ./pyx #'` collapses to one
    // dash-prefixed token `operands`' own filter would strip, round-3 review
    // finding 3), so unquoted `$'` alone is reason enough not to trust
    // whatever the tokenizer made of the rest of the line. A bare
    // `IFS=$'\n'` never reaches this point at all (it is a pure-assignment
    // stage), and `printf $'a\n'`/`echo $'x'`/`grep $'\t' file` are exempt
    // the same way any other `NAME_ONLY_PROGRAMS` member is, having already
    // returned above.
    if contains_unquoted_ansi_c_quote(stage_text) {
        return vec![RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        }];
    }

    let names_an_interpreter = slice.tokens[1..]
        .iter()
        .any(|tok| token_names_an_interpreter(tok, ctx.trusted_aliases));
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
    // (issue #384/#430) — stopping at the first one, as this used
    // to, let a path-like flag value (`setsid -u ./notes.txt ./pyx`) shadow
    // the real script that followed it, resolving to nothing and leaving
    // the actual target unexamined. `resolve`/`resolve_for_analysis` still
    // decide per candidate whether it is real (verified shebang) or nothing
    // (missing, directory, no shebang), so a benign command with several
    // path-like operands (`cp ./a.txt ./b.txt`, `tar -cf ./out.tar
    // ./srcdir`) stays exactly as auto-approved as a single candidate was.
    // A URL operand (`wget -O out http://x/y`) contains a `/` but names no
    // local file, so it is never a candidate.
    //
    // Only a `/` the shell writes itself makes an operand path-like. A `/`
    // inside a command substitution belongs to the substituted command's own
    // text (`--body "$(cat <<'EOF' ... docs/x.md ... EOF)"`), which the
    // wrapper walk routes on its own. So candidates come from the raw stage
    // with every real substitution masked to `$()` before the quotes go:
    // `"$(pwd)/pyx"` keeps its outer `/` and still degrades below, and a
    // quoted `'$(x/pyx)'` is literal text that keeps its `/`. An operand
    // whose only `/` sits inside a substitution joins the slash-less
    // `setsid $SCRIPT` gap. A substitution that never closes leaves the
    // unmasked tokens to decide.
    let masked_tokens = match aegis_parser::mask_command_substitutions(stage_text) {
        Some(std::borrow::Cow::Owned(masked)) => aegis_parser::split_tokens(&masked),
        _ => Vec::new(),
    };
    let masked_refs: Vec<&str> = masked_tokens.iter().map(String::as_str).collect();
    let masked_slice = aegis_parser::effective_token_slices(&masked_refs)
        .into_iter()
        .next();
    let candidate_tokens = masked_slice
        .as_ref()
        .map_or(&slice.tokens[1..], |masked| &masked.tokens[1..]);

    let mut seen = std::collections::HashSet::new();
    for token in candidate_tokens {
        // A separate-token executor option value (`-o VALUE`, `-c
        // KEY=VALUE`, `-P VALUE`) already produced its own route above
        // through the program-specific value grammar that knows how to
        // pick a command's leading word out of it — reading the same raw
        // token again here would treat its *whole*, unparsed text as a
        // second, literal candidate (`ssh -o 'ProxyCommand ./evil.py %h'
        // host` must candidate on `./evil.py` once, not also on the literal
        // string `"ProxyCommand ./evil.py %h"`, issue #384/#430).
        if option_consumed.contains(token) {
            continue;
        }
        // An operand that would itself assign or remove HOME (`eval
        // "HOME=/tmp/e"`) is exactly the shape decision D1 exists to catch,
        // not an ordinary path — checked against the *whole* token, ahead of
        // the D4 value extraction below, so stripping the `HOME=` prefix off
        // never hides it (GHSA-xj54).
        if token_is_home_assignment(token) {
            return vec![RoutedTarget::Unresolved {
                reason: DegradationReason::DynamicSource,
            }];
        }
        // A D4-shaped token (`--use-compress-program=./evil.py`,
        // `SHELL=./evil.sh`) candidates on its *value*, whatever the token's
        // own leading `-` says (Rule B, issue #384/#430).
        if let Some(value) = d4_value(token) {
            if let ControlFlow::Break(degraded) =
                record_path_candidate(value, &mut seen, home, ctx, &mut targets)
            {
                return degraded;
            }
            continue;
        }
        if !token.starts_with('-') {
            if let ControlFlow::Break(degraded) =
                record_path_candidate(token, &mut seen, home, ctx, &mut targets)
            {
                return degraded;
            }
            continue;
        }
        // A long option (`--pager`) with no `=` carries no glued value to
        // read — stays excluded exactly as before.
        if token.starts_with("--") {
            continue;
        }
        // A single-dash flag bundle can glue its value straight onto its
        // own letters with nothing marking where the flag ends and the
        // value begins (`-I./pyx`, `-cI./pyx`, `-e./pyx`, `-P./pyx`,
        // GHSA-xj54 follow-up): routing does not know which of a wrapper
        // program's short flags take an argument, so every split of the
        // leading letter run is its own candidate. Degrades at once on the
        // first split that names an interpreter; a bundle ahead of the real
        // letter run (`-cI./pyx`'s `c`) can produce a spurious earlier split
        // (`I./pyx`) alongside the real one (`./pyx`) — `route_path_like_
        // candidate`/`resolve` treat that the same as any other nonexistent
        // path, so it costs nothing beyond an extra candidate that resolves
        // to no target.
        //
        // A split only counts as naming an interpreter when its value is
        // more than one word (GHSA-xj54 follow-up): a linker or include
        // flag's own argument (`-lpython3.12`, `-lnode`, `-Ipython3`) is a
        // single word that never runs anything, and this same guessing walk
        // reads it as a split of the flag-letter run the same way it reads
        // a real glued value, so without this check `gcc -lpython3.12`
        // would degrade on `python3.12` alone. A quoted multi-word value
        // (`-I'python3 ./evil.py'`) still degrades — its first word is a
        // command line the flag hands to a real interpreter, not an inert
        // linker argument.
        for suffix in glued_short_flag_values(token) {
            if suffix.contains(char::is_whitespace)
                && token_names_an_interpreter(suffix, ctx.trusted_aliases)
            {
                return vec![RoutedTarget::Unresolved {
                    reason: DegradationReason::DynamicSource,
                }];
            }
            if let ControlFlow::Break(degraded) =
                record_path_candidate(suffix, &mut seen, home, ctx, &mut targets)
            {
                return degraded;
            }
        }
    }
    targets
}

/// The outcome of checking one path-like candidate string against
/// [`route_path_like_candidate`]'s own dedup/URL/degrade rules — shared by
/// the plain-operand and glued-short-flag candidate shapes in
/// [`unclaimed_interpreter_net`]'s per-token walk so the two reuse one
/// decision instead of two near-identical copies (GHSA-xj54 follow-up).
enum CandidateOutcome {
    /// Not a candidate at all: no `/`, a URL, or already seen this stage.
    Skip,
    /// Names a known interpreter or otherwise degrades; the caller returns
    /// at once with this as the stage's sole target.
    Degrade(RoutedTarget),
    /// A verified path-like candidate to add to this stage's targets.
    Add(RoutedTarget),
}

/// Applies the "contains `/`, not a URL, then reduce to the leading word,
/// not already seen this stage, then [`route_path_like_candidate`]" rule to
/// `candidate`. The `/`/URL gate runs on `candidate` as a whole — a
/// candidate carrying no `/` anywhere is not path-like whatever its later
/// words hold (`gzip -9`, `ssh -p 22`), and one whose danger lives in a word
/// the reduction would otherwise drop (`"$(echo x/pyx)"`, still literal text
/// since it reached here quoted) must still reach
/// [`route_path_like_candidate`]'s own [`is_literal_path`] check rather than
/// being read as clean just because its now-`/`-less leading word passes the
/// gate on its own. The leading-word reduction itself is
/// [`command_value_leading_word`] — the same one [`route_executor_value`]
/// already applies to a dispatched program's own option value (`man -P`,
/// `git -c core.pager=`) — so a several-word candidate from any of this
/// file's other channels (a glued short-flag split, a `d4_value` split, or a
/// plain operand) gets that exact same "first word, past any
/// `COMMAND_STRING_PREFIX_WORDS`/assignment run" treatment instead of being
/// read whole, spaces and all, as one literal (and so ordinarily
/// nonexistent) path (GHSA-xj54 follow-up): `tar -I'./pyx -d' -cf out.tar
/// dir` candidates on `./pyx`, not the literal `"./pyx -d"`.
fn evaluate_path_candidate<'a>(
    candidate: &'a str,
    seen: &mut std::collections::HashSet<&'a str>,
    home: HomeState,
    ctx: &RouteContext<'_>,
) -> CandidateOutcome {
    if !candidate.contains('/') || candidate.contains("://") {
        return CandidateOutcome::Skip;
    }
    let candidate = command_value_leading_word(candidate);
    if !seen.insert(candidate) {
        return CandidateOutcome::Skip;
    }
    match route_path_like_candidate(candidate, home, ctx) {
        routed @ RoutedTarget::Unresolved { .. } => CandidateOutcome::Degrade(routed),
        routed => CandidateOutcome::Add(routed),
    }
}

/// [`evaluate_path_candidate`]'s outcome applied to `targets`: `Add` pushes
/// onto it and keeps the stage's own operand walk going, `Skip` does
/// nothing, and `Degrade` returns the stage's sole target — the three call
/// sites in [`unclaimed_interpreter_net`]'s per-token walk (a D4 value, a
/// plain operand, a glued short-flag split) shared this same match arm by
/// arm until this helper collected it in one place.
fn record_path_candidate<'a>(
    candidate: &'a str,
    seen: &mut std::collections::HashSet<&'a str>,
    home: HomeState,
    ctx: &RouteContext<'_>,
    targets: &mut Vec<RoutedTarget>,
) -> ControlFlow<Vec<RoutedTarget>> {
    match evaluate_path_candidate(candidate, seen, home, ctx) {
        CandidateOutcome::Skip => ControlFlow::Continue(()),
        CandidateOutcome::Degrade(routed) => ControlFlow::Break(vec![routed]),
        CandidateOutcome::Add(routed) => {
            targets.push(routed);
            ControlFlow::Continue(())
        }
    }
}

/// Every non-empty suffix of `token` past 1..=N letters of its maximal
/// leading run of ASCII letters after the leading `-` (`-cI./pyx` past its
/// own `-` reads as `cI./pyx`, letter run `cI`, suffixes `I./pyx` and
/// `./pyx`) — the set of places a short-flag bundle glued directly onto its
/// own value could split into "flag letters" and "value", since routing
/// does not know which of a wrapper program's short flags take an argument
/// (GHSA-xj54 follow-up). Only meaningful for a single-dash token with no
/// `=`; a long-option or `=`-glued shape is [`d4_value`]'s own case instead.
/// Empty for a token that is just `-` followed by a single letter and
/// nothing else (`-o` alone carries no glued value to split out).
fn glued_short_flag_values(token: &str) -> Vec<&str> {
    let Some(rest) = token.strip_prefix('-') else {
        return Vec::new();
    };
    let letters = rest.chars().take_while(char::is_ascii_alphabetic).count();
    (1..=letters)
        .map(|split| &rest[split..])
        .filter(|suffix| !suffix.is_empty())
        .collect()
}

/// Route a single path-like word — an unclaimed stage's own operand, or the
/// value of an [`env_prefix_executor_route`]/[`option_value_executor_route`]
/// executor value (Rule B) — to a `LauncherOperand`, or degrade it in place:
/// a word that would itself assign or remove HOME (`eval "HOME=/tmp/e"`) is
/// exactly the shape decision D1 exists to catch, not an ordinary path
/// (GHSA-xj54); `~/rest` expands only while `home == HomeState::Trusted` and
/// a caller-supplied home exists — `~//pyx` is `$HOME//pyx`, so joining an
/// absolute `/pyx` onto the home would drop the home entirely; anything else
/// that fails [`super::super::is_literal_path`] (`$`, backtick, glob,
/// `~user`) degrades too.
fn route_path_like_candidate(word: &str, home: HomeState, ctx: &RouteContext<'_>) -> RoutedTarget {
    if token_is_home_assignment(word) {
        return RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        };
    }

    let path = if let Some(rest) = word.strip_prefix("~/").map(|r| r.trim_start_matches('/')) {
        match ctx
            .home
            .filter(|_| is_literal_path(rest) && home == HomeState::Trusted)
        {
            Some(home_dir) => home_dir.join(rest),
            None => {
                return RoutedTarget::Unresolved {
                    reason: DegradationReason::DynamicSource,
                };
            }
        }
    } else if is_literal_path(word) {
        PathBuf::from(word)
    } else {
        return RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        };
    };
    RoutedTarget::LauncherOperand { path }
}

/// The word an executor value actually runs: the whole `value` when it
/// carries no embedded whitespace, or its first word past any leading
/// [`COMMAND_STRING_PREFIX_WORDS`]/assignment run otherwise — the same
/// "opening word" [`text_names_an_interpreter`] reads for its own
/// multi-word check, reused here so a command-string executor value
/// (`ProxyCommand ./evil.py %h`) candidates on `./evil.py`, not the whole
/// string (Rule B, issue #384/#430).
fn command_value_leading_word(value: &str) -> &str {
    if value.contains(char::is_whitespace) {
        skip_command_string_prefix_words(value)
            .next()
            .unwrap_or(value)
    } else {
        value
    }
}

/// `Some` when an executor value — an option argument or an executor
/// environment variable's right-hand side — names a known interpreter (a
/// degrading `Unresolved`, as before) or is otherwise path-like (a
/// `LauncherOperand`, Rule B): a `LESSOPEN`/`ProxyCommand`/`core.pager`-style
/// value that runs a script by path gets the same shebang check `setsid
/// ./pyx` does, rather than being silently left as unrouted data just
/// because it sits behind an option or environment variable instead of a
/// bare stage operand. `None` when `value` names no interpreter and carries
/// no `/` (or is a URL) — ordinary data, same as today.
pub(super) fn route_executor_value(
    value: &str,
    home: HomeState,
    ctx: &RouteContext<'_>,
) -> Option<RoutedTarget> {
    let stripped = value
        .strip_prefix('|')
        .or_else(|| value.strip_prefix('!'))
        .unwrap_or(value);
    if token_names_an_interpreter(stripped, ctx.trusted_aliases) {
        return Some(RoutedTarget::Unresolved {
            reason: DegradationReason::DynamicSource,
        });
    }
    let candidate = command_value_leading_word(stripped);
    if !candidate.contains('/') || candidate.contains("://") {
        return None;
    }
    Some(route_path_like_candidate(candidate, home, ctx))
}

/// Words a shell accepts ahead of the program it actually runs without
/// changing what that program is: `exec` (replaces the shell with the
/// program instead of forking it), `command`/`builtin` (force builtin/PATH
/// resolution), `nohup`/`time`/`nice` (wrap execution, still run the word
/// that follows), and a bare `NAME=value` assignment (sets the environment
/// for the one command that follows it). A quoted multi-word command string
/// handed to an unenumerated wrapper (`script -c "exec python3 ./evil.py"`,
/// issue #384/#430, review comment 4091038690) can open with any number of
/// these before the interpreter it actually runs.
const COMMAND_STRING_PREFIX_WORDS: &[&str] =
    &["exec", "command", "builtin", "nohup", "time", "nice", "env"];

/// `tok`'s own words with a leading run of [`COMMAND_STRING_PREFIX_WORDS`]
/// entries and bare `NAME=value` assignments skipped, so a caller reaches
/// the word that actually runs (issue #384/#430, review comment
/// 4091038690). `env NAME=value` — the launcher word immediately followed
/// by its own assignment — is peeled one word at a time by the same loop
/// that peels a bare assignment, since both leave `env`'s own remaining
/// flags/assignments/program for the next iteration.
fn skip_command_string_prefix_words(tok: &str) -> std::str::SplitWhitespace<'_> {
    let mut words = tok.split_whitespace();
    loop {
        let mut lookahead = words.clone();
        let Some(word) = lookahead.next() else {
            break;
        };
        if COMMAND_STRING_PREFIX_WORDS.contains(&word)
            || word
                .split_once('=')
                .is_some_and(|(name, _)| is_shell_identifier(name))
        {
            words = lookahead;
        } else {
            break;
        }
    }
    words
}

/// `true` when `tok` itself names a known registry interpreter, or — for a
/// token that survived unquoting with embedded whitespace still in it (a
/// quoted multi-word argument, e.g. `"python3 ./evil.py"`, or
/// `"exec python3 ./evil.py"`, passed to `script -c`) — the first word past
/// any leading [`COMMAND_STRING_PREFIX_WORDS`]/assignment run does.
/// `basename` alone mishandles the quoted case: run on the whole token it
/// finds the last `/`-separated segment of the *last* word instead of the
/// interpreter name that opens it (issue #384/#430).
pub(super) fn token_names_an_interpreter(tok: &str, trusted_aliases: &[(&str, &str)]) -> bool {
    text_names_an_interpreter(tok, trusted_aliases)
        || d4_value(tok).is_some_and(|value| text_names_an_interpreter(value, trusted_aliases))
}

/// The value half of a `prefix=value` token whose `prefix` is non-empty and
/// carries no whitespace (`--use-compress-program=./evil.py`,
/// `SHELL=./evil.sh`) — D4, the generic assignment/long-option/short-option/
/// dotted-key form [`token_names_an_interpreter`] checks alongside its own
/// whole-token match, and [`unclaimed_interpreter_net`]'s path-like operand
/// walk reads the same way for Rule B (issue #384/#430): the candidate is
/// this value, not the whole `prefix=value` token, so `make SHELL=./evil.sh`
/// resolves to `./evil.sh`, not `SHELL=./evil.sh`. Keeps unwrapping while the
/// value it just peeled off still splits the same way (GHSA-xj54 follow-up):
/// `tar`'s `--checkpoint-action=exec=./pyx` nests a second `NAME=value` pair
/// inside the first one's own value, so a single split would stop at
/// `exec=./pyx` — a value that carries no `/` of its own and so never reads
/// as path-like — and lose the real target hiding one layer further in.
fn d4_value(tok: &str) -> Option<&str> {
    let (name, mut value) = tok.split_once('=')?;
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    while let Some((inner_name, inner_value)) = value.split_once('=') {
        if inner_name.is_empty() || inner_name.contains(char::is_whitespace) {
            break;
        }
        value = inner_value;
    }
    Some(value)
}

fn text_names_an_interpreter(text: &str, trusted_aliases: &[(&str, &str)]) -> bool {
    resolve_interpreter(basename(text), trusted_aliases).is_some()
        || (text.contains(char::is_whitespace)
            && skip_command_string_prefix_words(text)
                .next()
                .is_some_and(|word| resolve_interpreter(basename(word), trusted_aliases).is_some()))
}
