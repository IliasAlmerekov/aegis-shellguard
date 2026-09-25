#![deny(missing_docs)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

//! Shell command parsing for Aegis.
//!
//! This crate owns the tokenizer (quote/escape-aware splitting, heredoc and
//! inline-script extraction, pipeline segmentation, nested-shell unwrapping) and
//! the token-level `PrefixPattern` matcher. It produces the canonical
//! [`ParsedCommand`] consumed by the scanner. It depends only on `aegis-types`.

mod embedded_scripts;
mod env_launcher;
mod git_options;
mod heredoc_data_consumer;
mod list_segments;
mod nested_shells;
mod prefix_match;
mod runner;
mod segmentation;
mod tokenizer;

#[cfg(test)]
use aegis_types::InlineScript;
use aegis_types::ParsedCommand;
pub use embedded_scripts::{
    HeredocBody, extract_eval_payloads, extract_heredoc_bodies, extract_inline_scripts,
    extract_process_substitution_bodies, mask_inert_heredoc_substitution_markers,
    split_at_heredoc_marker,
};
use env_launcher::{env_prefix_lengths, env_split_string_tokens};
pub use git_options::{
    GitSubcommandStarts, MAX_GIT_OPTION_CANDIDATES, git_option_subcommand_starts,
};
pub use list_segments::{ListSegment, ListSeparator, list_segments};
pub use nested_shells::extract_nested_commands;
pub use prefix_match::{contains_any_token, matches_prefix};
pub use runner::Runner;
pub use segmentation::{
    extract_command_substitution_bodies, logical_segments, mask_command_substitutions,
    top_level_pipelines, unwrap_subshell_group,
};
pub use tokenizer::{extract_prefix, split_tokens};

/// Reserved words that keep the *next* word in command position too (issue
/// #384) — `!`/`if`/`then`/`elif`/`else`/`while`/`until`/`do`/`time` all
/// introduce a command rather than being one themselves. The single source
/// every command-position/reserved-word scan in this crate, and callers
/// outside it, reads instead of each keeping its own copy to drift out of
/// sync (issue #384/#430). `case`, `function`, and `coproc` are deliberately
/// absent: each needs handling beyond a flat membership check (nesting nested
/// `case`/`esac`, a header's own name and body, a two-word start), done
/// separately at the call sites that need it.
pub const COMMAND_STARTING_KEYWORDS: [&str; 9] = [
    "!", "if", "then", "elif", "else", "while", "until", "do", "time",
];

/// A token slice resolved to the program that prefix-style detection should use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveTokenSlice<'a> {
    /// The token sequence with the effective program in position 0.
    pub tokens: Vec<&'a str>,
    /// The basename-normalized program token used as an index key.
    pub program: &'a str,
}

/// Resolve candidate token slices after stripping known launcher prefixes.
///
/// This is detection-only normalization: it never changes the parsed command or
/// the command that will be executed. Absolute program paths are reduced to
/// their basename, and launchers such as `sudo`, `env`, `rtk`, `timeout`, and
/// `command` are skipped recursively so token-prefix rules see the program they
/// are meant to protect.
pub fn effective_token_slices<'a>(tokens: &[&'a str]) -> Vec<EffectiveTokenSlice<'a>> {
    effective_token_slices_checked(tokens).0
}

/// [`effective_token_slices`]'s companion for a caller that must tell a
/// depth-bounded stop apart from an ordinary "nothing resolved" result: the
/// same slices, paired with `true` when [`ENV_SPLIT_MAX_DEPTH`] cut an
/// `env -S`/`--split-string` chain short before it reached the real program
/// (#437 review, adversarial finding F1). A caller that would otherwise
/// treat an empty or partial slice list as a non-program stage must check
/// the flag first and fail closed instead — see `CONVENTION.md` §2
/// (classification and policy failures must be fail-closed). Computes both
/// in one resolution walk rather than making the caller run
/// [`effective_token_slices`] and a second truncation query separately.
pub fn effective_token_slices_checked<'a>(
    tokens: &[&'a str],
) -> (Vec<EffectiveTokenSlice<'a>>, bool) {
    if tokens.is_empty() {
        return (Vec::new(), false);
    }

    let (starts, owned, truncated) = effective_program_resolution(tokens);
    let mut slices = Vec::with_capacity(starts.len() + owned.len());
    for start in starts {
        let Some(effective_tokens) = effective_tokens_at(tokens, start) else {
            continue;
        };
        let program = effective_tokens[0];
        slices.push(EffectiveTokenSlice {
            tokens: effective_tokens,
            program,
        });
    }
    for effective_tokens in owned {
        let Some(&program) = effective_tokens.first() else {
            continue;
        };
        slices.push(EffectiveTokenSlice {
            tokens: effective_tokens,
            program,
        });
    }
    (slices, truncated)
}

/// Resolve the basename-normalized program token used for detection matching.
///
/// This is the allocation-free companion to [`effective_token_slices`] for call
/// sites that only need the lookup key, not a rewritten token slice.
pub fn effective_program<'a>(tokens: &[&'a str]) -> Option<&'a str> {
    let (starts, owned, _truncated) = effective_program_resolution(tokens);
    if let Some(&start) = starts.first() {
        return tokens.get(start).map(|token| program_basename(token));
    }
    owned.into_iter().next()?.into_iter().next()
}

/// `true` when `tok` *starts* with a redirection glyph (`<`, `>`, or one
/// prefixed by a leading fd — a bare digit (`2>`) or a bash named fd
/// (`{fd}>`) — optionally followed by `&` for the combined-stream form
/// (`&>`)), whether standalone (`>`, `2>`) or with a glued target/fd
/// (`>out`, `2>&1`, `<file`, `{fd}>out`, `1>|out`). Broader than
/// [`is_redirection_operator`], which only matches the pure-operator form.
///
/// The single source of truth for this decision, reused by both effective-
/// program resolution here and the router's own argv walk (issue #384).
#[must_use]
pub fn starts_with_redirection_glyph(tok: &str) -> bool {
    let after_fd = strip_leading_fd_marker(tok);
    let after_amp = after_fd.strip_prefix('&').unwrap_or(after_fd);
    after_amp.starts_with('<') || after_amp.starts_with('>')
}

/// A standalone shell redirection operator token (`>`, `>>`, `<`, `2>`, …) —
/// an fd marker (see [`starts_with_redirection_glyph`]) followed by nothing
/// but `<`/`>` characters. A glued form (`>out.txt`, `2>&1`, `>|out`) is not
/// standalone — it carries its own target in the same token and needs no
/// extra token skipped, so it is deliberately excluded here.
#[must_use]
pub fn is_redirection_operator(tok: &str) -> bool {
    let after_fd = strip_leading_fd_marker(tok);
    let after_amp = after_fd.strip_prefix('&').unwrap_or(after_fd);
    !after_amp.is_empty() && after_amp.chars().all(|c| c == '<' || c == '>')
}

/// Strip a leading redirection fd marker: a run of ASCII digits (`2>`), or a
/// bash named fd in braces (`{fd}>`, `{myfd}>`). Neither shape is valid
/// shell syntax as-is (a `{name}` fd requires the following `<`/`>` to make
/// it one), so this only strips the marker text itself and leaves that
/// check to the caller.
fn strip_leading_fd_marker(tok: &str) -> &str {
    if let Some(rest) = tok.strip_prefix('{')
        && let Some(close) = rest.find('}')
        && let name = &rest[..close]
        && !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return &rest[close + 1..];
    }
    tok.trim_start_matches(|c: char| c.is_ascii_digit())
}

/// Number of tokens a leading redirection at `tok` consumes: `2` for a
/// standalone operator (`>`, `2>`, …), whose target is the *next* token, or
/// `1` for a glued form (`>out`, `2>&1`, `{fd}>out`), which carries its
/// target/fd in the same token.
fn redirection_token_len(tok: &str) -> usize {
    if is_redirection_operator(tok) { 2 } else { 1 }
}

/// Ceiling on `env -S`/`--split-string` nesting (`env -S 'env -S ...'`, or
/// the flat `env -S 'env -S' 'env -S' ... 'true'` shape that re-splits into
/// the same form one repeat shorter each time): each level re-splits its own
/// value and feeds it back into [`collect_effective_program_indices`], so
/// without a bound a deep enough chain costs one stack frame per level with
/// nothing else to stop it (#437 review, adversarial finding F1 — a 3000-deep
/// chain overflowed the stack under a constrained `ulimit -s`). Matches the
/// router's `MAX_WRAP_DEPTH` (`src/analysis/router/segments.rs`) and
/// `OrchestrationBudget::L1_DEFAULT.max_depth` (ADR-022 §7's cross-language
/// recursion-depth ceiling) in value; duplicated as its own constant rather
/// than shared because this crate is a dependency-DAG leaf and may not
/// depend on the root `aegis` binary crate that owns those (CONVENTION.md
/// §3).
const ENV_SPLIT_MAX_DEPTH: u32 = 8;

/// Resolve effective-program candidates for `tokens` as two channels plus a
/// truncation flag: `starts` are indices into `tokens` itself, `owned` are
/// fully-resolved token vectors produced by an `env -S`/`--split-string`
/// expansion somewhere during recursive launcher-prefix stripping, and the
/// `bool` is `true` when [`ENV_SPLIT_MAX_DEPTH`] cut resolution short before
/// it reached the real program (#437 review, finding F1). A split's words
/// are new tokens, not a sub-range of `tokens` (review 4091038674 on PR
/// #437), so they cannot be expressed as an index back into it.
fn effective_program_resolution<'a>(tokens: &[&'a str]) -> (Vec<usize>, Vec<Vec<&'a str>>, bool) {
    let mut starts = Vec::new();
    let mut owned = Vec::new();
    let mut truncated = false;
    collect_effective_program_indices(tokens, 0, &mut starts, &mut owned, 0, &mut truncated);
    starts.sort_unstable();
    starts.dedup();
    (starts, owned, truncated)
}

fn collect_effective_program_indices<'a>(
    tokens: &[&'a str],
    index: usize,
    starts: &mut Vec<usize>,
    owned: &mut Vec<Vec<&'a str>>,
    depth: u32,
    truncated: &mut bool,
) {
    if index >= tokens.len() {
        return;
    }

    // A redirection can sit between assignments, launcher words, and the
    // program (`FOO=1 >out python3 x.py`, `env >out python3 x.py`) — the
    // shell strips it before argv0 resolution the same way it does a
    // leading one, so this check runs first at every recursive step, not
    // only at index 0 (issue #384).
    if starts_with_redirection_glyph(tokens[index]) {
        let len = redirection_token_len(tokens[index]).min(tokens.len() - index);
        collect_effective_program_indices(tokens, index + len, starts, owned, depth, truncated);
        return;
    }

    let assignment_prefix_len = leading_environment_assignment_prefix_len(&tokens[index..]);
    if assignment_prefix_len > 0 {
        collect_effective_program_indices(
            tokens,
            index + assignment_prefix_len,
            starts,
            owned,
            depth,
            truncated,
        );
        return;
    }

    // `env -S`/`--split-string` must be checked at every recursive step, not
    // only before recursion starts — a launcher word ahead of `env`
    // (`FOO=1 env -S ...`, `sudo env -S ...`) reaches this point mid-
    // recursion, and generic `env_prefix_lengths` does not know split-string
    // semantics (review 4091038674 on PR #437). A split value that is itself
    // `env -S ...` feeds back into `collect_effective_program_slices`, one
    // level deeper each time; past `ENV_SPLIT_MAX_DEPTH`, stop and record
    // the truncation instead of recursing further (#437 review, finding F1).
    if let Some(split) = env_split_string_tokens(&tokens[index..]) {
        if depth >= ENV_SPLIT_MAX_DEPTH {
            *truncated = true;
            return;
        }
        collect_effective_program_slices(&split, owned, depth + 1, truncated);
        return;
    }

    match launcher_prefix_lengths(&tokens[index..]) {
        Some(lengths) => {
            for len in lengths {
                if len == 0 {
                    starts.push(index);
                } else {
                    collect_effective_program_indices(
                        tokens,
                        index + len,
                        starts,
                        owned,
                        depth,
                        truncated,
                    );
                }
            }
        }
        None => starts.push(index),
    }
}

/// Resolve `tokens` — already `env -S`-split argv, not a sub-range of any
/// outer token array — into effective-program token vectors, continuing
/// recursive launcher-prefix stripping (and a further split, should the
/// split content itself start with another `env -S`) on the split content.
/// `depth` is this split's own nesting level, checked against
/// [`ENV_SPLIT_MAX_DEPTH`] by [`collect_effective_program_indices`] before
/// it recurses back in here.
fn collect_effective_program_slices<'a>(
    tokens: &[&'a str],
    owned: &mut Vec<Vec<&'a str>>,
    depth: u32,
    truncated: &mut bool,
) {
    let mut starts = Vec::new();
    collect_effective_program_indices(tokens, 0, &mut starts, owned, depth, truncated);
    for start in starts {
        if let Some(effective_tokens) = effective_tokens_at(tokens, start) {
            owned.push(effective_tokens);
        }
    }
}

/// Build the effective token vector for the program at `start` in `tokens`:
/// the program's basename in position 0, followed by the remaining tokens
/// unchanged. Shared by the index-based and `env -S`-split resolution paths.
fn effective_tokens_at<'a>(tokens: &[&'a str], start: usize) -> Option<Vec<&'a str>> {
    let program = program_basename(tokens.get(start)?);
    let mut effective_tokens = Vec::with_capacity(tokens.len().saturating_sub(start));
    effective_tokens.push(program);
    effective_tokens.extend(tokens[start + 1..].iter().copied());
    Some(effective_tokens)
}

/// Return the number of leading shell environment assignments.
///
/// The shell consumes these words before it resolves the command program, so
/// effective-program detection must do the same. Restrict the name to the
/// portable shell identifier grammar so an ordinary argument containing `=`
/// never disappears from matching.
fn leading_environment_assignment_prefix_len(tokens: &[&str]) -> usize {
    tokens
        .iter()
        .take_while(|token| is_environment_assignment(token))
        .count()
}

pub(crate) fn is_environment_assignment(token: &str) -> bool {
    let Some((name, _value)) = token.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn launcher_prefix_lengths(tokens: &[&str]) -> Option<Vec<usize>> {
    let launcher = program_basename(tokens.first().copied()?);
    if launcher.eq_ignore_ascii_case("rtk") {
        return Some(rtk_prefix_lengths(tokens));
    }

    if launcher.eq_ignore_ascii_case("nohup")
        || launcher.eq_ignore_ascii_case("time")
        || launcher.eq_ignore_ascii_case("command")
        || launcher.eq_ignore_ascii_case("doas")
        || launcher.eq_ignore_ascii_case("exec")
        || launcher.eq_ignore_ascii_case("builtin")
        || launcher.eq_ignore_ascii_case("coproc")
    {
        return Some(vec![1]);
    }

    if launcher.eq_ignore_ascii_case("timeout") {
        return Some(timeout_prefix_lengths(tokens));
    }

    if launcher.eq_ignore_ascii_case("nice") {
        return Some(vec![nice_prefix_len(tokens)]);
    }

    if launcher.eq_ignore_ascii_case("sudo") {
        return Some(sudo_prefix_lengths(tokens));
    }

    if launcher.eq_ignore_ascii_case("env") {
        return Some(env_prefix_lengths(tokens));
    }

    if launcher.eq_ignore_ascii_case("xargs") {
        return Some(vec![xargs_prefix_len(tokens)]);
    }

    if let Some(prefix_len) =
        Runner::from_program(launcher).and_then(|runner| runner.command_prefix_len(tokens))
    {
        return Some(vec![prefix_len]);
    }

    None
}

/// Resolve the launcher-prefix length for bare `xargs <program> <args...>`
/// (issue #384): with no placeholder/replacement flag, `xargs` runs
/// `<program> <args...>` directly (plus stdin lines appended as further
/// arguments), so the program right after its own flags is the effective
/// program the same way `sudo`'s or `nice`'s is.
fn xargs_prefix_len(tokens: &[&str]) -> usize {
    let mut index = 1;
    while index < tokens.len() {
        let token = tokens[index];
        if token == "--" {
            index += 1;
            break;
        }
        if !token.starts_with('-') {
            break;
        }
        index += 1;
        if matches!(
            token,
            "-a" | "--arg-file"
                | "-d"
                | "--delimiter"
                | "-E"
                | "-I"
                | "-i"
                | "--replace"
                | "-L"
                | "--max-lines"
                | "-l"
                | "-n"
                | "--max-args"
                | "-P"
                | "--max-procs"
                | "-s"
                | "--max-chars"
        ) && index < tokens.len()
        {
            index += 1;
        }
    }
    index
}

/// Resolve the launcher-prefix length for the `rtk` wrapper.
///
/// `rtk <cmd>` transparently forwards `<cmd>`, so stripping the single `rtk`
/// token exposes the real program (issue #339's baseline case). `rtk proxy
/// <cmd>` is different: `proxy` is rtk's own meta-command for running `<cmd>`
/// unfiltered, so `<cmd>` starts one token further in. Without this
/// distinction `rtk proxy git rebase origin/main` resolves its effective
/// program to `proxy`, which matches no rule and is auto-approved safe with
/// no snapshot — a full detection bypass for the identical underlying
/// command.
fn rtk_prefix_lengths(tokens: &[&str]) -> Vec<usize> {
    match tokens.get(1) {
        Some(sub) if sub.eq_ignore_ascii_case("proxy") => vec![2],
        _ => vec![1],
    }
}

fn sudo_prefix_lengths(tokens: &[&str]) -> Vec<usize> {
    sudo_prefix_lengths_from(tokens, 1)
}

fn sudo_prefix_lengths_from(tokens: &[&str], mut index: usize) -> Vec<usize> {
    while index < tokens.len() {
        let token = tokens[index];
        if token.contains('=') {
            index += 1;
            continue;
        }
        if !token.starts_with('-') || token == "-" {
            break;
        }
        index += 1;
        if matches!(
            token,
            "-u" | "--user"
                | "-g"
                | "--group"
                | "-h"
                | "--host"
                | "-p"
                | "--prompt"
                | "-C"
                | "--close-from"
                | "-T"
                | "--command-timeout"
        ) && index < tokens.len()
        {
            index += 1;
        } else if index < tokens.len() {
            let mut candidates = sudo_prefix_lengths_from(tokens, index);
            if index < tokens.len() {
                candidates.push(index + 1);
            }
            candidates.sort_unstable();
            candidates.dedup();
            return candidates;
        }
    }
    vec![index]
}

fn timeout_prefix_lengths(tokens: &[&str]) -> Vec<usize> {
    let mut index = 1;
    while index < tokens.len() {
        let token = tokens[index];
        if token == "--" {
            index += 1;
            break;
        }
        if matches!(
            token,
            "-v" | "--verbose" | "--foreground" | "--preserve-status"
        ) || token.starts_with("--signal=")
            || token.starts_with("--kill-after=")
            || short_timeout_option_with_value(token)
        {
            index += 1;
            continue;
        }
        if matches!(token, "-s" | "--signal" | "-k" | "--kill-after") {
            index += 2.min(tokens.len() - index);
            continue;
        }
        if token.starts_with('-') {
            return if index + 1 < tokens.len() {
                vec![index + 1, index + 2]
            } else {
                vec![index + 1]
            };
        }
        break;
    }
    vec![(index + 1).min(tokens.len())]
}

fn short_timeout_option_with_value(token: &str) -> bool {
    token.len() > 2 && (token.starts_with("-s") || token.starts_with("-k"))
}

fn nice_prefix_len(tokens: &[&str]) -> usize {
    if tokens.len() > 2 && matches!(tokens[1], "-n" | "--adjustment") {
        3
    } else if tokens.len() > 1 && tokens[1].starts_with('-') && tokens[1].len() > 1 {
        2
    } else {
        1
    }
}

pub(crate) fn program_basename(token: &str) -> &str {
    if let Some((_, basename)) = token.rsplit_once('/') {
        basename
    } else {
        token
    }
}

/// One top-level segment within a pipeline chain.
///
/// `raw` preserves the original shell spelling for diagnostics, while
/// `normalized` joins shell tokens with single spaces so downstream matching can
/// reason about neighboring pipeline stages without quote noise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineSegment {
    /// Original shell spelling of this segment.
    pub raw: String,
    /// Shell tokens joined by single spaces (no quoting noise).
    pub normalized: String,
}

/// A top-level shell pipeline chain such as `cmd1 | cmd2 | cmd3`.
///
/// Chains are delimited only by top-level control operators other than the
/// single pipe (`;`, `&&`, `||`, newlines). This preserves adjacency between
/// neighboring pipeline stages for semantic analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineChain {
    /// Original shell spelling of the full chain.
    pub raw: String,
    /// Individual pipeline stages within the chain.
    pub segments: Vec<PipelineSegment>,
}

/// A stateless parser that converts raw shell command strings into [`ParsedCommand`].
pub struct Parser;

impl Parser {
    /// Parse `cmd` into a [`ParsedCommand`].
    ///
    /// Tokenizes `cmd` (respecting quoting and escaping), then extracts the
    /// program name and argument list from the first logical command. The full
    /// token sequence is joined into `normalized` — the canonical match target
    /// used by the scanner. The raw string is preserved only for audit logging.
    pub fn parse(cmd: &str) -> ParsedCommand {
        let tokens = split_tokens(cmd);

        // Tokens of the first sub-command only (before any shell separator).
        let first_cmd: Vec<&String> = tokens
            .iter()
            .take_while(|t| !matches!(t.as_str(), ";" | "&&" | "||" | "|"))
            .collect();

        let program = first_cmd.first().map(|s| s.to_string());
        let argv: Vec<String> = first_cmd.iter().skip(1).map(|s| s.to_string()).collect();

        // De-quoted, space-joined form of the full token sequence.
        let normalized = tokens.join(" ");

        let inline_scripts = extract_inline_scripts(cmd);

        ParsedCommand {
            program,
            argv,
            normalized,
            inline_scripts,
            raw: cmd.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    mod parsing_tests;
    mod runner_tests;
    mod segments_logical_tests;
    mod tokenizer_tests;

    #[test]
    fn effective_token_slices_strip_launchers_and_absolute_paths() {
        let tokens = [
            "sudo",
            "env",
            "FOO=bar",
            "rtk",
            "/usr/bin/git",
            "reset",
            "--hard",
        ];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].program, "git");
        assert_eq!(slices[0].tokens, vec!["git", "reset", "--hard"]);
    }

    #[test]
    fn effective_token_slices_strip_shell_launchers() {
        for launcher in ["exec", "builtin", "coproc"] {
            let tokens = [launcher, "git", "push", "--force"];
            let slices = effective_token_slices(&tokens);
            assert_eq!(slices[0].program, "git", "{launcher}");
            assert_eq!(slices[0].tokens, vec!["git", "push", "--force"]);
        }
    }

    #[test]
    fn effective_token_slices_strip_xargs_launcher() {
        let tokens = ["xargs", "python3", "./evil.py"];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].program, "python3");
        assert_eq!(slices[0].tokens, vec!["python3", "./evil.py"]);
    }

    #[test]
    fn effective_token_slices_strip_xargs_flags_before_the_program() {
        let tokens = ["xargs", "-n1", "python3", "./evil.py"];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices[0].program, "python3");
        assert_eq!(slices[0].tokens, vec!["python3", "./evil.py"]);
    }

    #[test]
    fn effective_token_slices_strip_rtk_proxy_meta_command() {
        let tokens = ["rtk", "proxy", "git", "rebase", "origin/main"];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].program, "git");
        assert_eq!(slices[0].tokens, vec!["git", "rebase", "origin/main"]);
    }

    #[test]
    fn effective_token_slices_strip_rtk_proxy_meta_command_case_insensitively() {
        let tokens = ["rtk", "PROXY", "git", "rebase", "origin/main"];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].program, "git");
        assert_eq!(slices[0].tokens, vec!["git", "rebase", "origin/main"]);
    }

    #[test]
    fn effective_token_slices_treat_rtk_own_subcommand_as_program() {
        let tokens = ["rtk", "gain", "--history"];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].program, "gain");
        assert_eq!(slices[0].tokens, vec!["gain", "--history"]);
    }

    #[test]
    fn effective_token_slices_skip_launcher_options() {
        let tokens = [
            "sudo",
            "-u",
            "root",
            "timeout",
            "5s",
            "/bin/kill",
            "-9",
            "1",
        ];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices[0].program, "kill");
        assert_eq!(slices[0].tokens, vec!["kill", "-9", "1"]);
    }

    #[test]
    fn effective_token_slices_parse_timeout_options() {
        let tokens = [
            "timeout",
            "-s",
            "KILL",
            "-k",
            "10s",
            "5s",
            "/usr/bin/git",
            "reset",
            "--hard",
        ];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices[0].program, "git");
        assert_eq!(slices[0].tokens, vec!["git", "reset", "--hard"]);
    }

    #[test]
    fn effective_token_slices_keep_conservative_unknown_sudo_flag_candidates() {
        let tokens = ["sudo", "--new-opt", "value", "git", "reset", "--hard"];
        let slices = effective_token_slices(&tokens);
        let programs: Vec<&str> = slices.iter().map(|slice| slice.program).collect();

        assert!(programs.contains(&"value"));
        assert!(programs.contains(&"git"));
    }

    #[test]
    fn effective_token_slices_keep_scanning_after_unknown_sudo_flags() {
        let tokens = ["sudo", "-n", "-u", "postgres", "psql", "-c", "DROP TABLE t"];
        let slices = effective_token_slices(&tokens);
        let programs: Vec<&str> = slices.iter().map(|slice| slice.program).collect();

        assert!(programs.contains(&"psql"));
    }

    #[test]
    fn effective_token_slices_skip_sudo_environment_assignment() {
        let tokens = ["sudo", "FOO=bar", "git", "reset", "--hard"];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices[0].program, "git");
        assert_eq!(slices[0].tokens, vec!["git", "reset", "--hard"]);
    }

    #[test]
    fn effective_token_slices_skip_leading_environment_assignments() {
        let tokens = [
            "CARGO_TARGET_DIR=/tmp/aegis",
            "FOO=x",
            "git",
            "reset",
            "--hard",
        ];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices[0].program, "git");
        assert_eq!(slices[0].tokens, vec!["git", "reset", "--hard"]);
    }

    #[test]
    fn effective_token_slices_keeps_non_shell_assignment_as_a_program() {
        let tokens = ["1FOO=not-a-command", "echo", "hello"];
        let slices = effective_token_slices(&tokens);

        assert_eq!(slices[0].program, "1FOO=not-a-command");
        assert_eq!(
            slices[0].tokens,
            vec!["1FOO=not-a-command", "echo", "hello"]
        );
    }
}
