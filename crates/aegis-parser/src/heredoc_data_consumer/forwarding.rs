//! The forwarding-only allowlist behind [`super::assignment_variable_runs_later`]
//! (issue #396 capture-then-execute, ADR-042 item 5): once a nowdoc is
//! captured into a shell variable (`NAME=$(cat <<'EOF' ...)`), this decides
//! whether every later use of that variable is provably a forward — the
//! value of a data flag on `gh`/`git`/`curl`/`jq`, or any argument of
//! `echo`/`printf` (except printf's own format string) — never something
//! that re-runs the text or hands it to a program that reads it as a file
//! path or a script (issue #396 review follow-up: an "any argument" rule
//! was too wide, since `jq -n`, `gh --input`/`-F`, and `curl -T`/`-K`/`-o`/a
//! bare URL each turn a forwarded value into something other than inline
//! data). Split out of [`super`] to keep it under the 800-line budget in
//! `tests/file_size_budget.rs`.

use crate::extract_process_substitution_bodies;
use crate::segmentation::split_top_level_segments;
use crate::split_tokens;

use super::{MESSAGE_FLAGS, is_bare_assignment, open_frames, starts_word};

/// Programs whose only trusted use of a captured heredoc variable is
/// forwarding it through a data flag ([`segment_is_forwarding_only`]'s
/// per-program checks) — as an argument, or (for `git`) as a commit/tag/issue
/// message value. Nothing else: not the program itself, not an interpreter,
/// not a wrapper like `sudo`/`env`/`time`/`nohup`/`command`/`find -exec`
/// (ADR-042 item 5, issue #396 allowlist replacing the old blocklist).
const TRUSTED_FORWARDING_PROGRAMS: &[&str] = &["gh", "git", "curl", "echo", "printf", "jq"];

/// `gh` flags whose value curl-style file semantics never apply to: a commit
/// or tag message, an issue or PR title or body — reuses [`MESSAGE_FLAGS`],
/// the same set `git` trusts, since `gh`'s `-m`/`--message`, `-t`/`--title`
/// and `-b`/`--body` line up with it exactly.
const GH_MESSAGE_FLAGS: &[&str] = MESSAGE_FLAGS;

/// `curl` flags whose value is always inline data, whatever the captured
/// heredoc body's first line looks like.
const CURL_ALWAYS_TRUSTED_DATA_FLAGS: &[&str] = &["--data-raw"];

/// `curl` flags whose value is inline data only while the captured heredoc
/// body does not start with `@` — curl reads an `@`-prefixed value as a file
/// path to upload instead (issue #396 review: `curl -d "$x" url` where `$x`
/// captured `@/etc/shadow` would post that file's contents, not the
/// heredoc's own text).
const CURL_CONDITIONAL_DATA_FLAGS: &[&str] = &[
    "-d",
    "--data",
    "--data-binary",
    "--data-urlencode",
    "--json",
];

/// Bare words that, anywhere in `following_text`, can run a captured
/// heredoc variable through indirection an argument-position scan would
/// never see: `eval`/`source`/a bare `.` re-parse text as code, `alias`
/// stores a command for later, `trap` stores a signal handler, and
/// `PROMPT_COMMAND` runs on every prompt. Checked as whole words (split on
/// anything that is not an ASCII alphanumeric or `_`, matching
/// [`heredoc_marker_context`]'s [`UNTRUSTED_PREFIX_WORDS`] check) so a glued
/// form such as `PROMPT_COMMAND=$x` still trips it.
const INDIRECTION_WORDS: &[&str] = &["eval", "source", "alias", "trap", "PROMPT_COMMAND"];

/// Commands that only add nameref indirection (`${!ref}` resolving through
/// `ref`) when combined with `-n` (`declare -n ref=name`, `local -n`,
/// `typeset -n`). Checked together with a literal `-n` occurring anywhere in
/// the same text rather than requiring the two adjacent, which is
/// conservative but never lets a real `declare -n` through unflagged.
const NAMEREF_COMMANDS: &[&str] = &["declare", "local", "typeset"];

/// `true` when `following_text` contains any of rule 1's indirection
/// shapes, checked independently of whether a literal `$NAME`/`${NAME`
/// reference appears at all — a nameref (`declare -n r=x; $r`) or an
/// `alias`/`trap`/`PROMPT_COMMAND` store never spells the variable's own
/// name as `$NAME`, so [`assignment_variable_runs_later`]'s reference scan
/// alone would miss them.
fn following_text_has_indirection(following_text: &str) -> bool {
    if following_text.contains("${!") || following_text.contains("<<<") {
        return true;
    }
    let has_word = |word: &str| {
        following_text
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .any(|token| token == word)
    };
    if INDIRECTION_WORDS.iter().any(|word| has_word(word)) {
        return true;
    }
    if NAMEREF_COMMANDS.iter().any(|word| has_word(word)) && following_text.contains("-n") {
        return true;
    }
    bare_dot_command(following_text)
}

/// `true` when `text` has a standalone `.` shell word — the `source`
/// builtin's alias — bounded by a word start before it and whitespace, `;`,
/// `&`, `|`, or end of text right after, so `./script.sh` and a filename
/// like `release.tar` are left alone.
fn bare_dot_command(text: &str) -> bool {
    text.char_indices().any(|(idx, ch)| {
        ch == '.'
            && starts_word(text, idx)
            && text[idx + 1..]
                .chars()
                .next()
                .is_none_or(|next| next.is_whitespace() || matches!(next, ';' | '&' | '|'))
    })
}

/// Byte offsets of the `$` that starts every occurrence of `$name` or any
/// `${name...}` parameter expansion in `text`, boundary-checked so a longer
/// name never matches a shorter one (`$xerox` does not match `name = "x"`).
/// Covers every shape the Problem section's probes use: the bare `$x`
/// command word, `${x}`, `${x:-}`, `${x%%foo}`.
fn find_variable_references(text: &str, name: &str) -> Vec<usize> {
    let is_identifier_char = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut positions = Vec::new();
    let mut search_from = 0;
    while let Some(rel) = text[search_from..].find('$') {
        let pos = search_from + rel;
        let after_dollar = pos + 1;
        let rest = &text[after_dollar..];
        let name_start = if let Some(braced) = rest.strip_prefix('{') {
            braced.starts_with(name).then_some(after_dollar + 1)
        } else {
            rest.starts_with(name).then_some(after_dollar)
        };
        if let Some(start) = name_start {
            let end = start + name.len();
            let boundary_ok = text[end..]
                .chars()
                .next()
                .is_none_or(|c| !is_identifier_char(c));
            if boundary_ok {
                positions.push(pos);
                search_from = end;
                continue;
            }
        }
        search_from = pos + 1;
    }
    positions
}

/// `true` when `segment` has an unquoted `>` that is not one of rule 2's two
/// allowed redirects (`2>&1`, `>&2`) — any other target can send a trusted
/// program's output to a file or a descriptor this checker cannot vouch for.
fn segment_has_disallowed_redirect(segment: &str) -> bool {
    let chars: Vec<char> = segment.chars().collect();
    let mut single_quote = false;
    let mut double_quote = false;
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\\' if !single_quote => i += 1,
            '\'' if !double_quote => single_quote = !single_quote,
            '"' if !single_quote => double_quote = !double_quote,
            '>' if !single_quote && !double_quote => {
                let is_2_to_1 = i > 0
                    && chars[i - 1] == '2'
                    && chars.get(i + 1) == Some(&'&')
                    && chars.get(i + 2) == Some(&'1');
                let is_to_2 = chars.get(i + 1) == Some(&'&') && chars.get(i + 2) == Some(&'2');
                if !is_2_to_1 && !is_to_2 {
                    return true;
                }
            }
            _ => {}
        }
        i += 1;
    }
    false
}

/// `true` when `token` is exactly `$name` or `${name}` — the whole token is
/// the reference, nothing else before or after it.
fn is_whole_reference(token: &str, name: &str) -> bool {
    token == format!("${name}") || token == format!("${{{name}}}")
}

/// `true` when `args[index]` is the value of a flag `trusted` accepts:
/// standalone (`--flag "$x"`, so the previous token is the flag) or glued
/// (`--flag=$x`). Shared by every [`TRUSTED_FORWARDING_PROGRAMS`] member
/// whose data flags take the reference as their whole value — `git`'s
/// message flags and `gh`'s [`GH_MESSAGE_FLAGS`]/curl's data flags alike.
/// `gh`'s `-f`/`--raw-field` does not fit this shape (its value is a
/// `key=value` pair, not the reference alone) and uses
/// [`token_is_gh_raw_field_value`] instead.
fn token_is_flag_value(
    args: &[String],
    index: usize,
    name: &str,
    trusted: impl Fn(&str) -> bool,
) -> bool {
    let token = &args[index];
    if let Some((flag, value)) = token.split_once('=') {
        return trusted(flag) && is_whole_reference(value, name);
    }
    is_whole_reference(token, name) && index > 0 && trusted(args[index - 1].as_str())
}

/// `true` when `token` is `KEY=value` with a non-empty `KEY` and `value` the
/// whole reference to `name` — the shape a `gh api -f KEY="$x"` value takes,
/// since `-f`/`--raw-field`'s value is a `key=value` pair rather than the
/// reference on its own.
fn token_is_key_equals_reference(token: &str, name: &str) -> bool {
    token
        .split_once('=')
        .is_some_and(|(key, value)| !key.is_empty() && is_whole_reference(value, name))
}

/// `true` when `args[index]` is the `key=$x` value of a `gh`
/// `-f`/`--raw-field` flag: standalone (`-f key=$x`, previous token is the
/// flag) or glued to the long form (`--raw-field=key=$x`). Never
/// `-F`/`--field` — that flag's value can carry a `@filename` prefix gh
/// resolves to a file's contents, which `-f`/`--raw-field` never does
/// (issue #396 review).
fn token_is_gh_raw_field_value(args: &[String], index: usize, name: &str) -> bool {
    let token = &args[index];
    if index > 0
        && matches!(args[index - 1].as_str(), "-f" | "--raw-field")
        && token_is_key_equals_reference(token, name)
    {
        return true;
    }
    token
        .strip_prefix("--raw-field=")
        .is_some_and(|rest| token_is_key_equals_reference(rest, name))
}

/// `true` when `args[index]` (a token after `git`'s program word) is the
/// value of a [`MESSAGE_FLAGS`] flag.
fn token_is_git_message_value(args: &[String], index: usize, name: &str) -> bool {
    token_is_flag_value(args, index, name, |flag| MESSAGE_FLAGS.contains(&flag))
}

/// `true` when every reference to `name` in `gh`'s `args` is the value of
/// [`GH_MESSAGE_FLAGS`] or a `-f`/`--raw-field` pair
/// ([`token_is_gh_raw_field_value`]). Not `-F`/`--field` (typed, and its
/// value can be a `@filename` gh reads instead of sending literally), not
/// `--input`/`--body-file` (both name a file to read), not a positional
/// argument.
fn gh_args_are_forwarding_only(args: &[String], name: &str) -> bool {
    if matches!(
        args.first().map(String::as_str),
        Some("alias" | "extension")
    ) {
        return false;
    }
    args.iter().enumerate().all(|(i, token)| {
        find_variable_references(token, name).is_empty()
            || token_is_flag_value(args, i, name, |flag| GH_MESSAGE_FLAGS.contains(&flag))
            || token_is_gh_raw_field_value(args, i, name)
    })
}

/// `true` when every reference to `name` in `curl`'s `args` is the value of
/// [`CURL_ALWAYS_TRUSTED_DATA_FLAGS`] or, while `body_starts_with_at` is
/// `false`, [`CURL_CONDITIONAL_DATA_FLAGS`]. Not `-T`, `-K`, `-o`,
/// `--config`, `-F`/`--form`, or a URL position — each of those can turn a
/// captured value into a file curl reads from or writes to, rather than data
/// it sends as-is.
fn curl_args_are_forwarding_only(args: &[String], name: &str, body_starts_with_at: bool) -> bool {
    let trusted = |flag: &str| {
        CURL_ALWAYS_TRUSTED_DATA_FLAGS.contains(&flag)
            || (!body_starts_with_at && CURL_CONDITIONAL_DATA_FLAGS.contains(&flag))
    };
    args.iter().enumerate().all(|(i, token)| {
        find_variable_references(token, name).is_empty()
            || token_is_flag_value(args, i, name, trusted)
    })
}

/// `true` when `args[index]` is the value token of a `--arg NAME value` or
/// `--argjson NAME value` triple: the reference itself, two positions after
/// the flag word.
fn token_is_jq_arg_value(args: &[String], index: usize, name: &str) -> bool {
    is_whole_reference(&args[index], name)
        && index >= 2
        && matches!(args[index - 2].as_str(), "--arg" | "--argjson")
}

/// `true` when every reference to `name` in `jq`'s `args` is the value token
/// of a `--arg`/`--argjson` pair ([`token_is_jq_arg_value`]). Not a
/// positional argument (jq's program text, run against the input), not
/// `-f`/`--from-file` (reads the program from a file), not
/// `--rawfile`/`--slurpfile` (both read a file's contents into a variable).
fn jq_args_are_forwarding_only(args: &[String], name: &str) -> bool {
    args.iter().enumerate().all(|(i, token)| {
        find_variable_references(token, name).is_empty() || token_is_jq_arg_value(args, i, name)
    })
}

/// `true` when every reference to `name` in `printf`'s `args` sits at index
/// 1 or later — anywhere but `args[0]`, the format string that shapes how
/// printf reads every argument after it rather than being forwarded as-is.
/// A captured format string cannot run a shell command, only mis-format the
/// output, but it is not a plain forwarded value either, so it stays on the
/// untrusted side rather than being waved through by analogy with `echo`.
fn printf_args_are_forwarding_only(args: &[String], name: &str) -> bool {
    args.iter()
        .enumerate()
        .all(|(i, token)| find_variable_references(token, name).is_empty() || i != 0)
}

/// `true` when every use of `name`'s reference inside `segment` — already a
/// single top-level simple command, [`split_top_level_segments`]'s unit —
/// is provably forwarding-only (ADR-042 item 5's allowlist, rules 2 and 3):
///
/// - no reference is nested inside a `(`, `{`, `$(`, a backtick, `<(`, or
///   `>(` still open at that point in `segment` ([`open_frames`]);
/// - no reference sits in a leading `NAME=value` assignment run ahead of the
///   segment's program (`FOO=$x gh ...` launders the reference through
///   `FOO`, so it counts as assigning it to another variable);
/// - the segment has no disallowed redirect
///   ([`segment_has_disallowed_redirect`]);
/// - the segment's program (basename-normalized) is a
///   [`TRUSTED_FORWARDING_PROGRAMS`] member — a wrapper (`sudo`), an
///   interpreter, or a compound-command keyword (`then`, `do`, ...) is
///   never one, which is also how a bare `y=$x` (no program token at all)
///   and a segment whose *only* token is the reference itself
///   (`${x:-}` as a whole command) both fail here;
/// - the reference must be the value of that program's own data flag, not
///   just any argument ([`gh_args_are_forwarding_only`],
///   [`curl_args_are_forwarding_only`], [`jq_args_are_forwarding_only`],
///   [`printf_args_are_forwarding_only`], [`token_is_git_message_value`]).
///   `echo` alone keeps the old "any argument" rule, since it has no flag
///   that reads a file or another program's text instead of printing the
///   value verbatim.
///
/// `body_starts_with_at` is the captured heredoc's own first body line,
/// trimmed of leading whitespace, starting with `@` — the one fact
/// [`curl_args_are_forwarding_only`] needs that isn't visible from
/// `segment` alone, since curl reads an `@`-prefixed data value as a file
/// path instead of sending it literally.
fn segment_is_forwarding_only(segment: &str, name: &str, body_starts_with_at: bool) -> bool {
    for pos in find_variable_references(segment, name) {
        if !open_frames(&segment[..pos]).is_empty() {
            return false;
        }
    }
    if segment_has_disallowed_redirect(segment) {
        return false;
    }
    let tokens = split_tokens(segment);
    let Some(program_pos) = tokens.iter().position(|token| !is_bare_assignment(token)) else {
        return false;
    };
    if tokens[..program_pos]
        .iter()
        .any(|token| !find_variable_references(token, name).is_empty())
    {
        return false;
    }
    let program = &tokens[program_pos];
    let basename = program
        .rsplit_once('/')
        .map_or(program.as_str(), |(_, tail)| tail);
    if !TRUSTED_FORWARDING_PROGRAMS
        .iter()
        .any(|candidate| basename.eq_ignore_ascii_case(candidate))
    {
        return false;
    }
    let args = &tokens[program_pos + 1..];
    if basename.eq_ignore_ascii_case("gh") {
        return gh_args_are_forwarding_only(args, name);
    }
    if basename.eq_ignore_ascii_case("git") {
        return args.iter().enumerate().all(|(i, token)| {
            find_variable_references(token, name).is_empty()
                || token_is_git_message_value(args, i, name)
        });
    }
    if basename.eq_ignore_ascii_case("curl") {
        return curl_args_are_forwarding_only(args, name, body_starts_with_at);
    }
    if basename.eq_ignore_ascii_case("jq") {
        return jq_args_are_forwarding_only(args, name);
    }
    if basename.eq_ignore_ascii_case("printf") {
        return printf_args_are_forwarding_only(args, name);
    }
    true
}

/// `true` when `following_text` — every command line after a captured
/// heredoc's terminator — is provably forwarding-only for `name`, the
/// variable a `NAME=$(cat <<'EOF' ...)` capture just assigned (issue #396
/// capture-then-execute, ADR-042 item 5). This is an allowlist, not a
/// blocklist: `following_text` counts as forwarding-only only when rule 1
/// finds no indirection anywhere ([`following_text_has_indirection`]),
/// no process-substitution body carries the reference, and every top-level
/// simple command that does carry it passes [`segment_is_forwarding_only`]
/// (`body_starts_with_at` threaded through for curl's data-flag check). A
/// chain that pipes to another stage alongside the reference is rejected
/// outright, conservatively — telling which stage feeds which would need a
/// real shell parser this predicate does not have.
fn following_text_is_forwarding_only(
    name: &str,
    following_text: &str,
    body_starts_with_at: bool,
) -> bool {
    if following_text_has_indirection(following_text) {
        return false;
    }
    if extract_process_substitution_bodies(following_text)
        .iter()
        .any(|body| !find_variable_references(body, name).is_empty())
    {
        return false;
    }
    split_top_level_chains(following_text)
        .into_iter()
        .all(|chain| {
            let segments = split_top_level_segments(chain);
            let referenced_in_chain = !find_variable_references(chain, name).is_empty();
            if segments.len() > 1 && referenced_in_chain {
                return false;
            }
            segments.iter().all(|segment| {
                find_variable_references(segment, name).is_empty()
                    || segment_is_forwarding_only(segment, name, body_starts_with_at)
            })
        })
}

/// `true` when `following_text` can run `name`, the variable a
/// `NAME=$(cat <<'EOF' ...)` capture just assigned (issue #396
/// capture-then-execute). Delegates to
/// [`following_text_is_forwarding_only`]'s allowlist and inverts it: a shape
/// that allowlist cannot clear falls on the untrusted side, matching
/// ADR-042's "a parsing gap costs a false positive, not a bypass".
/// `body_starts_with_at` is the captured heredoc's own first body line,
/// trimmed of leading whitespace, starting with `@` — see
/// [`segment_is_forwarding_only`] for why curl needs it.
pub(super) fn assignment_variable_runs_later(
    name: &str,
    following_text: &str,
    body_starts_with_at: bool,
) -> bool {
    if following_text.is_empty() {
        return false;
    }
    let dollar = format!("${name}");
    let braced_prefix = format!("${{{name}");
    let has_reference =
        following_text.contains(dollar.as_str()) || following_text.contains(braced_prefix.as_str());
    if !has_reference && !following_text_has_indirection(following_text) {
        return false;
    }
    !following_text_is_forwarding_only(name, following_text, body_starts_with_at)
}

/// Byte-range groups of `text` split at an unquoted `;`, `\n`, `&&`, or
/// `||`, honoring `$(`/backtick/`(`-nesting the same way
/// [`owning_simple_command_start`] does. A `|` or standalone `&` stays glued
/// to its group on purpose: [`assignment_variable_runs_later`] needs to tell
/// a piped group (where a reference anywhere in it counts as running the
/// variable) apart from a merely sequential one, and re-splitting a group
/// with [`split_top_level_segments`] — which does split on those — is how it
/// does that.
fn split_top_level_chains(text: &str) -> Vec<&str> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut chains = Vec::new();
    let mut start = 0usize;
    let mut single_quote = false;
    let mut double_quote = false;
    let mut in_backticks = false;
    let mut paren_depth = 0u32;
    let mut command_sub_depth = 0u32;
    let mut i = 0usize;

    while i < chars.len() {
        let (idx, ch) = chars[i];
        match ch {
            '\\' if !single_quote => i += 1,
            '\'' if !double_quote && !in_backticks => single_quote = !single_quote,
            '"' if !single_quote && !in_backticks => double_quote = !double_quote,
            '`' if !single_quote => in_backticks = !in_backticks,
            '$' if !single_quote && chars.get(i + 1).map(|&(_, c)| c) == Some('(') => {
                command_sub_depth += 1;
                i += 1;
            }
            '(' if !single_quote && !double_quote => paren_depth += 1,
            ')' if !single_quote && !double_quote => {
                if command_sub_depth > 0 {
                    command_sub_depth -= 1;
                } else {
                    paren_depth = paren_depth.saturating_sub(1);
                }
            }
            ';' | '\n'
                if !single_quote
                    && !double_quote
                    && !in_backticks
                    && paren_depth == 0
                    && command_sub_depth == 0 =>
            {
                chains.push(&text[start..idx]);
                start = idx + ch.len_utf8();
            }
            '&' | '|'
                if !single_quote
                    && !double_quote
                    && !in_backticks
                    && paren_depth == 0
                    && command_sub_depth == 0
                    && chars.get(i + 1).map(|&(_, c)| c) == Some(ch) =>
            {
                chains.push(&text[start..idx]);
                let (next_idx, next_ch) = chars[i + 1];
                start = next_idx + next_ch.len_utf8();
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    chains.push(&text[start..]);
    chains
}

#[cfg(test)]
mod tests {
    use super::{
        bare_dot_command, find_variable_references, following_text_has_indirection,
        is_whole_reference, segment_has_disallowed_redirect, segment_is_forwarding_only,
        split_top_level_chains,
    };

    #[test]
    fn find_variable_references_matches_bare_dollar_name() {
        assert_eq!(find_variable_references("echo $x", "x"), vec![5]);
    }

    #[test]
    fn find_variable_references_skips_a_longer_name_sharing_the_prefix() {
        // `$xy` names a different variable than `x`; a naive substring
        // match would wrongly count it as a reference to `x`.
        assert!(find_variable_references("echo $xy", "x").is_empty());
    }

    #[test]
    fn find_variable_references_matches_braced_form() {
        assert_eq!(find_variable_references("echo ${x}", "x"), vec![5]);
    }

    #[test]
    fn find_variable_references_matches_braced_form_with_a_default() {
        assert_eq!(find_variable_references("echo ${x:-a}", "x"), vec![5]);
    }

    #[test]
    fn find_variable_references_skips_a_longer_braced_name() {
        assert!(find_variable_references("echo ${xy}", "x").is_empty());
    }

    #[test]
    fn find_variable_references_finds_every_occurrence() {
        assert_eq!(find_variable_references("$x $x", "x"), vec![0, 3]);
    }

    #[test]
    fn bare_dot_command_true_for_a_standalone_dot() {
        assert!(bare_dot_command(". script.sh"));
    }

    #[test]
    fn bare_dot_command_false_for_a_relative_script_path() {
        assert!(!bare_dot_command("./script.sh"));
    }

    #[test]
    fn bare_dot_command_false_for_a_filename_with_a_dot() {
        assert!(!bare_dot_command("release.tar"));
    }

    #[test]
    fn following_text_has_indirection_true_for_the_eval_word() {
        assert!(following_text_has_indirection("eval \"$x\""));
    }

    #[test]
    fn following_text_has_indirection_false_for_a_word_only_sharing_a_substring() {
        // "resourceful" contains "source" as a substring but is not the
        // `source` builtin, so the whole-word check must leave it alone.
        assert!(!following_text_has_indirection("resourceful $x"));
    }

    #[test]
    fn following_text_has_indirection_true_for_a_flagged_nameref_declare() {
        assert!(following_text_has_indirection("declare -n r=x; $r"));
    }

    #[test]
    fn following_text_has_indirection_false_for_declare_without_the_nameref_flag() {
        assert!(!following_text_has_indirection("declare r=x; echo $r"));
    }

    #[test]
    fn segment_has_disallowed_redirect_false_for_the_two_allowed_forms() {
        assert!(!segment_has_disallowed_redirect(
            "git commit -m \"$x\" 2>&1"
        ));
        assert!(!segment_has_disallowed_redirect("git commit -m \"$x\" >&2"));
    }

    #[test]
    fn segment_has_disallowed_redirect_true_for_a_file_target() {
        assert!(segment_has_disallowed_redirect(
            "git commit -m \"$x\" > out.log"
        ));
    }

    #[test]
    fn is_whole_reference_true_for_bare_and_braced_forms() {
        assert!(is_whole_reference("$x", "x"));
        assert!(is_whole_reference("${x}", "x"));
    }

    #[test]
    fn is_whole_reference_false_when_the_token_carries_more_than_the_reference() {
        assert!(!is_whole_reference("\"$x\"", "x"));
        assert!(!is_whole_reference("$xy", "x"));
    }

    #[test]
    fn segment_is_forwarding_only_true_for_a_trusted_program_argument() {
        assert!(segment_is_forwarding_only("echo \"$x\"", "x", false));
    }

    #[test]
    fn segment_is_forwarding_only_false_for_an_untrusted_program() {
        assert!(!segment_is_forwarding_only("bash -c \"$x\"", "x", false));
    }

    #[test]
    fn segment_is_forwarding_only_false_when_the_reference_is_not_a_git_message_value() {
        assert!(!segment_is_forwarding_only(
            "git -c \"alias.x=!$x\" x",
            "x",
            false
        ));
    }

    // ── Issue #396 review: per-program data-flag narrowing ─────────────────

    #[test]
    fn segment_is_forwarding_only_false_for_jq_positional_argument() {
        assert!(!segment_is_forwarding_only("jq -n \"$x\"", "x", false));
    }

    #[test]
    fn segment_is_forwarding_only_false_for_jq_from_file_flag() {
        assert!(!segment_is_forwarding_only("jq -f \"$x\"", "x", false));
    }

    #[test]
    fn segment_is_forwarding_only_true_for_jq_arg_value() {
        assert!(segment_is_forwarding_only(
            "jq --arg b \"$x\" '{b:$b}'",
            "x",
            false
        ));
    }

    #[test]
    fn segment_is_forwarding_only_true_for_jq_argjson_value() {
        assert!(segment_is_forwarding_only(
            "jq --argjson b \"$x\" '{b:$b}'",
            "x",
            false
        ));
    }

    #[test]
    fn segment_is_forwarding_only_false_for_gh_input_flag() {
        assert!(!segment_is_forwarding_only(
            "gh api --input \"$x\" /repos/x/y/issues",
            "x",
            false
        ));
    }

    #[test]
    fn segment_is_forwarding_only_false_for_gh_field_flag() {
        assert!(!segment_is_forwarding_only(
            "gh api -F body=\"$x\" /repos/x/y/issues",
            "x",
            false
        ));
    }

    #[test]
    fn segment_is_forwarding_only_true_for_gh_raw_field_value() {
        assert!(segment_is_forwarding_only(
            "gh api -f body=\"$x\" /repos/x/y/issues",
            "x",
            false
        ));
    }

    #[test]
    fn segment_is_forwarding_only_true_for_gh_raw_field_glued_long_form() {
        assert!(segment_is_forwarding_only(
            "gh api --raw-field=body=$x /repos/x/y/issues",
            "x",
            false
        ));
    }

    #[test]
    fn segment_is_forwarding_only_false_for_curl_data_flag_when_body_starts_with_at() {
        assert!(!segment_is_forwarding_only(
            "curl -d \"$x\" https://example.com",
            "x",
            true
        ));
    }

    #[test]
    fn segment_is_forwarding_only_true_for_curl_data_flag_when_body_does_not_start_with_at() {
        assert!(segment_is_forwarding_only(
            "curl -d \"$x\" https://example.com",
            "x",
            false
        ));
    }

    #[test]
    fn segment_is_forwarding_only_true_for_curl_data_raw_even_when_body_starts_with_at() {
        assert!(segment_is_forwarding_only(
            "curl --data-raw \"$x\" https://example.com",
            "x",
            true
        ));
    }

    #[test]
    fn segment_is_forwarding_only_false_for_curl_upload_file_flag() {
        assert!(!segment_is_forwarding_only(
            "curl -T \"$x\" https://example.com",
            "x",
            false
        ));
    }

    #[test]
    fn segment_is_forwarding_only_false_for_curl_config_flag() {
        assert!(!segment_is_forwarding_only("curl -K \"$x\"", "x", false));
    }

    #[test]
    fn segment_is_forwarding_only_false_for_curl_output_flag() {
        assert!(!segment_is_forwarding_only(
            "curl -o \"$x\" https://example.com",
            "x",
            false
        ));
    }

    #[test]
    fn segment_is_forwarding_only_false_for_curl_url_position() {
        assert!(!segment_is_forwarding_only("curl \"$x\"", "x", false));
    }

    #[test]
    fn segment_is_forwarding_only_false_for_printf_format_string() {
        assert!(!segment_is_forwarding_only("printf \"$x\"", "x", false));
    }

    #[test]
    fn segment_is_forwarding_only_true_for_printf_later_argument() {
        assert!(segment_is_forwarding_only(
            "printf \"%s\" \"$x\"",
            "x",
            false
        ));
    }

    #[test]
    fn split_top_level_chains_splits_on_a_semicolon() {
        assert_eq!(split_top_level_chains("true; $x"), vec!["true", " $x"]);
    }

    #[test]
    fn split_top_level_chains_keeps_a_pipe_glued_to_its_chain() {
        // A lone `|` stays part of the same chain so the caller can tell a
        // piped reference apart from a merely sequential one.
        assert_eq!(
            split_top_level_chains("echo \"$x\" | sh"),
            vec!["echo \"$x\" | sh"]
        );
    }
}
