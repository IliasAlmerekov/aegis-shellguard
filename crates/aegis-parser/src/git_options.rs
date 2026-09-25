//! Git global-option skipping (GHSA-7564).
//!
//! `git`'s own options between `git` and its subcommand (`-C <path>`,
//! `-c <name>=<value>`, `--git-dir=...`, ...) shift the subcommand off
//! position 1, so a `GIT-*` rule keyed on `["git", "<subcommand>", ...]`
//! never sees it. This module resolves, for a token slice whose first token
//! is `git`, every position the subcommand could start at, so the scanner
//! can re-run its `GIT-*` rules against each candidate. It does not touch
//! [`crate::EffectiveTokenSlice`] or execution in any way. It is
//! detection-only, exactly like [`crate::effective_token_slices`].

use std::collections::VecDeque;

/// Options that take no value (`git(1)` 2.55's global options, checked
/// against local git 2.43.0 behaviour). Case-sensitive: git flags are.
const NO_VALUE_OPTIONS: &[&str] = &[
    "-v",
    "--version",
    "-h",
    "--help",
    "--html-path",
    "--man-path",
    "--info-path",
    "-p",
    "--paginate",
    "-P",
    "--no-pager",
    "--bare",
    "--no-replace-objects",
    "--no-lazy-fetch",
    "--no-optional-locks",
    "--no-advice",
    "--literal-pathspecs",
    "--glob-pathspecs",
    "--noglob-pathspecs",
    "--icase-pathspecs",
];

/// Options whose value is glued in with `=`, or is the next token when this
/// one carries no `=`.
const EQ_OR_NEXT_TOKEN_OPTIONS: &[&str] = &[
    "--git-dir",
    "--work-tree",
    "--namespace",
    "--config-env",
    "--attr-source",
];

/// Cap on distinct subcommand-start candidates one git slice may produce.
///
/// Each candidate costs the scanner a full token-prefix and regex rescan
/// (`crates/aegis-scanner/src/scanner/assessment.rs`), so a chain of `n`
/// unlisted options that each yield a candidate makes that per-slice cost
/// grow with `n`, and the total scan cost with `n^2`. A review measured 2000
/// unlisted options at 180ms against 100 at 0.83ms. Past this cap,
/// [`git_option_subcommand_starts`] reports [`GitSubcommandStarts::TooMany`]
/// instead of returning any candidate, so the scanner never pays that cost;
/// the caller reports a Warn of its own instead (GHSA-7564).
pub const MAX_GIT_OPTION_CANDIDATES: usize = 16;

/// Outcome of resolving where a git slice's subcommand could start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitSubcommandStarts {
    /// Every candidate start position, ascending, at most
    /// [`MAX_GIT_OPTION_CANDIDATES`] of them.
    Starts(Vec<usize>),
    /// The walk found more than [`MAX_GIT_OPTION_CANDIDATES`] candidates.
    /// The caller must not scan any of them.
    TooMany,
}

/// How many tokens a git global-option token consumes, starting at itself.
enum OptionArity {
    /// Consumes only this token.
    NoValue,
    /// Consumes this token and exactly the next one, unconditionally
    /// (`-C <path>`, `-c <name>[=<value>]`). Git has no `=`-glued form for
    /// either, so a token spelled `-C/tmp` or `-cfoo=bar` is not this case;
    /// see [`classify`].
    NextToken,
    /// Consumes this token and, only when it carries no `=`, the next one
    /// too.
    EqOrNextToken,
    /// Not a known option: two readings apply, see [`successors`].
    Unlisted,
}

/// Classify `token`, a candidate git global-option token already known to
/// start with `-`.
///
/// Glued short forms git rejects as unknown options (`-C/tmp`, `-cfoo=bar`)
/// are deliberately left unclassified here. Matching `-C`/`-c` demands an
/// exact token, so a glued spelling falls through to [`OptionArity::Unlisted`]
/// rather than being special-cased, per the settled option table.
fn classify(token: &str) -> OptionArity {
    if token == "-C" || token == "-c" {
        return OptionArity::NextToken;
    }

    let name = token.split('=').next().unwrap_or(token);

    // `--exec-path` takes an optional value glued with `=`; either spelling
    // consumes only this one token.
    if name == "--exec-path" {
        return OptionArity::NoValue;
    }

    // `--list-cmds=<group>` consumes only itself; the bare form is rejected
    // by git, so it falls to the unlisted-option rule instead.
    if name == "--list-cmds" {
        return if token.contains('=') {
            OptionArity::NoValue
        } else {
            OptionArity::Unlisted
        };
    }

    if NO_VALUE_OPTIONS.contains(&token) {
        return OptionArity::NoValue;
    }

    if EQ_OR_NEXT_TOKEN_OPTIONS.contains(&name) {
        return OptionArity::EqOrNextToken;
    }

    OptionArity::Unlisted
}

/// Positions reachable from `pos` (a git global-option token) in one step.
///
/// A known-arity option has exactly one successor. An unlisted option has
/// two: the position right after it (it took no value) and the position two
/// tokens after it (it took the next token as its value). That is the same
/// two-reading treatment ADR-014 uses for an unknown launcher flag. Either
/// successor is omitted when it would run past the end of `tokens`, per the
/// rule that a candidate with no remaining token produces nothing.
fn successors(tokens: &[&str], pos: usize) -> [Option<usize>; 2] {
    let has_next = pos + 1 < tokens.len();

    match classify(tokens[pos]) {
        OptionArity::NoValue => [Some(pos + 1), None],
        OptionArity::NextToken => [has_next.then_some(pos + 2), None],
        OptionArity::EqOrNextToken => {
            if tokens[pos].contains('=') {
                [Some(pos + 1), None]
            } else {
                [has_next.then_some(pos + 2), None]
            }
        }
        OptionArity::Unlisted => [Some(pos + 1), has_next.then_some(pos + 2)],
    }
}

/// Resolve every position `tokens` could start its subcommand at, once git
/// global options between `git` and the subcommand are skipped.
///
/// `tokens` is an already effective-program-resolved slice (`tokens[0]` is
/// `"git"`). Returns `GitSubcommandStarts::Starts(vec![])` when `tokens` has
/// fewer than two tokens or `tokens[1]` does not start with `-`, so a plain
/// `git reset --hard` never enters this path; the ordinary prefix match
/// already covers it.
///
/// Reachable positions are found by walking the option chain as a small
/// state graph. Each position is visited at most once, rather than building
/// every combination of readings: `n` unlisted options in a chain yield at
/// most `n` extra candidate positions, never `2^n` (GHSA-7564). The walk
/// still stops as soon as it has found more than [`MAX_GIT_OPTION_CANDIDATES`]
/// candidates, returning [`GitSubcommandStarts::TooMany`] at that point
/// rather than draining the rest of the queue, so cost past the cap stays
/// bounded by the cap itself, not by `tokens.len()`.
pub fn git_option_subcommand_starts(tokens: &[&str]) -> GitSubcommandStarts {
    if tokens.len() < 2 || !tokens[1].starts_with('-') {
        return GitSubcommandStarts::Starts(Vec::new());
    }

    let mut visited = vec![false; tokens.len()];
    let mut queue = VecDeque::new();
    let mut starts = Vec::new();

    visited[1] = true;
    queue.push_back(1);

    while let Some(pos) = queue.pop_front() {
        if !tokens[pos].starts_with('-') {
            starts.push(pos);
            if starts.len() > MAX_GIT_OPTION_CANDIDATES {
                return GitSubcommandStarts::TooMany;
            }
            continue;
        }

        for succ in successors(tokens, pos).into_iter().flatten() {
            if succ < tokens.len() && !visited[succ] {
                visited[succ] = true;
                queue.push_back(succ);
            }
        }
    }

    starts.sort_unstable();
    GitSubcommandStarts::Starts(starts)
}

#[cfg(test)]
mod tests {
    use super::{GitSubcommandStarts, MAX_GIT_OPTION_CANDIDATES, git_option_subcommand_starts};

    /// Unwraps the `Starts` variant; panics on `TooMany`, which none of the
    /// cases below except the dedicated cap tests should ever hit.
    fn starts(tokens: &[&str]) -> Vec<usize> {
        match git_option_subcommand_starts(tokens) {
            GitSubcommandStarts::Starts(starts) => starts,
            GitSubcommandStarts::TooMany => panic!("unexpected TooMany for {tokens:?}"),
        }
    }

    #[test]
    fn empty_and_single_token_slices_produce_nothing() {
        assert_eq!(starts(&[]), Vec::<usize>::new());
        assert_eq!(starts(&["git"]), Vec::<usize>::new());
    }

    #[test]
    fn plain_subcommand_produces_nothing() {
        // No leading `-`: the ordinary prefix match already covers this, so
        // the helper must skip the whole walk.
        assert_eq!(starts(&["git", "reset", "--hard"]), Vec::<usize>::new());
    }

    #[test]
    fn dash_capital_c_takes_the_next_token_as_its_value() {
        assert_eq!(starts(&["git", "-C", ".", "reset", "--hard"]), vec![3]);
    }

    #[test]
    fn dash_c_takes_the_next_token_as_its_value() {
        assert_eq!(starts(&["git", "-c", "core.editor=vim", "commit"]), vec![3]);
    }

    #[test]
    fn git_dir_takes_a_glued_or_separate_value() {
        assert_eq!(
            starts(&["git", "--git-dir=.git", "branch", "-D", "x"]),
            vec![2]
        );
        assert_eq!(
            starts(&["git", "--git-dir", ".git", "branch", "-D", "x"]),
            vec![3]
        );
    }

    #[test]
    fn work_tree_takes_the_next_token_when_bare() {
        assert_eq!(
            starts(&["git", "--work-tree", "x", "reset", "--hard"]),
            vec![3]
        );
    }

    #[test]
    fn no_value_options_consume_only_themselves() {
        assert_eq!(starts(&["git", "--no-pager", "log"]), vec![2]);
        assert_eq!(starts(&["git", "-P", "log"]), vec![2]);
    }

    #[test]
    fn chain_of_several_known_options_resolves_to_one_start() {
        let tokens = [
            "git",
            "-C",
            ".",
            "-c",
            "x=y",
            "--no-pager",
            "reset",
            "--hard",
        ];
        assert_eq!(starts(&tokens), vec![6]);
    }

    #[test]
    fn bare_list_cmds_falls_to_the_unlisted_rule() {
        // Git rejects a bare `--list-cmds`, so it gets the two-reading
        // unlisted treatment rather than a dedicated arity.
        let tokens = ["git", "--list-cmds", "reset", "--hard"];
        assert_eq!(starts(&tokens), vec![2]);
    }

    #[test]
    fn list_cmds_with_value_consumes_only_itself() {
        assert_eq!(
            starts(&["git", "--list-cmds=main", "reset", "--hard"]),
            vec![2]
        );
    }

    #[test]
    fn bare_exec_path_consumes_only_itself() {
        // Bare `--exec-path` prints the path and takes no value, so the next
        // token is the subcommand, not a value.
        assert_eq!(starts(&["git", "--exec-path", "reset", "--hard"]), vec![2]);
    }

    #[test]
    fn exec_path_with_glued_value_consumes_only_itself() {
        assert_eq!(
            starts(&["git", "--exec-path=/usr/lib/git-core", "reset", "--hard"]),
            vec![2]
        );
    }

    #[test]
    fn dangling_option_with_no_following_token_produces_nothing() {
        // `-C` and `--work-tree` demand a value; with none left, the
        // candidate produces nothing rather than guessing.
        assert_eq!(starts(&["git", "-C"]), Vec::<usize>::new());
        assert_eq!(starts(&["git", "--work-tree"]), Vec::<usize>::new());
    }

    #[test]
    fn print_and_exit_option_with_no_following_token_produces_nothing() {
        assert_eq!(starts(&["git", "--version"]), Vec::<usize>::new());
    }

    #[test]
    fn glued_dash_capital_c_short_form_is_treated_as_unlisted_and_still_finds_reset() {
        // Git rejects `-C/tmp` as an unknown option (`-C` demands an exact
        // token); it falls to the unlisted-option rule rather than being
        // read as `-C /tmp`, and `reset` is still found.
        assert_eq!(starts(&["git", "-C/tmp", "reset", "--hard"]), vec![2]);
    }

    #[test]
    fn glued_dash_c_short_form_is_treated_as_unlisted_and_still_finds_reset() {
        assert_eq!(starts(&["git", "-cfoo=bar", "reset", "--hard"]), vec![2]);
    }

    #[test]
    fn unlisted_option_yields_both_readings() {
        assert_eq!(starts(&["git", "--newopt", "reset", "--hard"]), vec![2]);
        assert_eq!(
            starts(&["git", "--newopt", "x", "reset", "--hard"]),
            vec![2, 3]
        );
    }

    #[test]
    fn unlisted_option_chain_yields_linear_not_exponential_candidates() {
        // A 2^n implementation of this walk would not finish within any
        // reasonable test timeout for 2000 unlisted options in a row; this
        // test passing at all is part of what pins the O(n) bound, not just
        // the length assertion below (GHSA-7564).
        let mut tokens: Vec<String> = vec!["git".to_string()];
        for i in 0..2000 {
            tokens.push(format!("--unknown-{i}"));
        }
        tokens.push("reset".to_string());
        tokens.push("--hard".to_string());
        let token_refs: Vec<&str> = tokens.iter().map(String::as_str).collect();

        let result = starts(&token_refs);

        assert!(
            result.len() <= token_refs.len(),
            "candidate count must stay linear in token count: got {} starts for {} tokens",
            result.len(),
            token_refs.len()
        );
    }

    /// Builds `["git", "--o0", "v0", "--o1", "v1", ..., "reset", "--hard"]`:
    /// `pairs` unlisted options each immediately followed by a plain-looking
    /// value, so each option's no-value reading lands on that value token
    /// and counts as one more candidate start (GHSA-7564 cap).
    fn alternating_unlisted_option_value_tokens(pairs: usize) -> Vec<String> {
        let mut tokens: Vec<String> = vec!["git".to_string()];
        for i in 0..pairs {
            tokens.push(format!("--o{i}"));
            tokens.push(format!("v{i}"));
        }
        tokens.push("reset".to_string());
        tokens.push("--hard".to_string());
        tokens
    }

    #[test]
    fn exactly_the_cap_worth_of_candidates_still_resolves() {
        let tokens = alternating_unlisted_option_value_tokens(MAX_GIT_OPTION_CANDIDATES - 1);
        let token_refs: Vec<&str> = tokens.iter().map(String::as_str).collect();

        // `MAX_GIT_OPTION_CANDIDATES - 1` value tokens plus the final
        // `reset` is exactly `MAX_GIT_OPTION_CANDIDATES` candidates.
        let result = starts(&token_refs);
        assert_eq!(result.len(), MAX_GIT_OPTION_CANDIDATES);
    }

    #[test]
    fn one_more_than_the_cap_reports_too_many() {
        let tokens = alternating_unlisted_option_value_tokens(MAX_GIT_OPTION_CANDIDATES);
        let token_refs: Vec<&str> = tokens.iter().map(String::as_str).collect();

        assert_eq!(
            git_option_subcommand_starts(&token_refs),
            GitSubcommandStarts::TooMany
        );
    }

    #[test]
    fn two_thousand_unlisted_option_value_pairs_report_too_many_fast() {
        // Each pair adds one candidate immediately, so the walk crosses the
        // cap within the first few pairs and must not touch the other
        // ~1990: this pins the O(cap) bound past the limit, not just the
        // TooMany outcome (GHSA-7564).
        let tokens = alternating_unlisted_option_value_tokens(2000);
        let token_refs: Vec<&str> = tokens.iter().map(String::as_str).collect();

        let started = std::time::Instant::now();
        let result = git_option_subcommand_starts(&token_refs);
        let elapsed = started.elapsed();

        assert_eq!(result, GitSubcommandStarts::TooMany);
        assert!(
            elapsed < std::time::Duration::from_millis(50),
            "must bail out at the cap, not walk all 2000 pairs: took {elapsed:?}"
        );
    }
}
