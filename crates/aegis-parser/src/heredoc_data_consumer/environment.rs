//! Whether the variable a `NAME=$(cat <<'EOF' ...)` capture assigns can run
//! without any later `$NAME` (PR #463 review). Git, bash, and editors read
//! some variables themselves: `GIT_SSH_COMMAND`, `GIT_EDITOR`, `PS4` under
//! `set -x`, `BASH_ENV` in a child bash. A denylist of such names would miss
//! the next one, so trust is limited to names with no uppercase letter,
//! which no shell or common tool reads as code, and that never reach a
//! child's environment through an env prefix or an export.

/// Builtins that can export a variable or give it an attribute. Any of them
/// in the text around the capture keeps the body scanned, whatever flags
/// follow, since `declare -x` and `local -x` export as `export` does.
const EXPORT_WORDS: &[&str] = &["declare", "export", "local", "typeset"];

/// `true` when `name`, just assigned from a captured nowdoc, may reach a
/// program that reads it as code without `following_text` naming it.
/// `prefix` is the command text before the marker and `following_text`
/// every command line after the heredoc's terminator, as
/// [`super::heredoc_target_is_data_consumer`] passes them.
pub(super) fn captured_variable_may_run_unnamed(
    name: &str,
    prefix: &str,
    following_text: &str,
) -> bool {
    name.bytes().any(|byte| byte.is_ascii_uppercase())
        || assignment_prefixes_command(following_text)
        || may_export(prefix)
        || may_export(following_text)
}

/// `true` unless the `$(...)` closes on the first line of `following_text`
/// and that line then ends or starts a new command. Anything else after
/// the `)` (`" git fetch`, another assignment, more of the same word) makes
/// the assignment an env prefix or leaves the shape unclear.
fn assignment_prefixes_command(following_text: &str) -> bool {
    let Some(rest) = following_text
        .trim_start_matches([' ', '\t', '\n'])
        .strip_prefix(')')
    else {
        return true;
    };
    let rest = rest.strip_prefix('"').unwrap_or(rest);
    let line = rest
        .split('\n')
        .next()
        .unwrap_or_default()
        .trim_start_matches([' ', '\t']);
    !(line.is_empty() || line.starts_with(';') || line.starts_with("&&") || line.starts_with("||"))
}

/// `true` when `text` has an [`EXPORT_WORDS`] word, mentions `allexport`,
/// or runs `set` with a short option cluster holding `a` (`set -a`,
/// `set -ea`, `set +a` is a false positive the check accepts). Quote
/// characters are dropped first, so a split word (`ex''port`) still counts.
fn may_export(text: &str) -> bool {
    let unquoted: String = text
        .chars()
        .filter(|c| !matches!(c, '\'' | '"' | '\\'))
        .collect();
    if unquoted.contains("allexport")
        || unquoted
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .any(|word| EXPORT_WORDS.contains(&word))
    {
        return true;
    }
    unquoted
        .split(['\n', ';', '&', '|', '(', ')', '{', '}', '`'])
        .any(|command| {
            let mut words = command
                .split_whitespace()
                .skip_while(|word| matches!(*word, "builtin" | "command"));
            words.next() == Some("set")
                && words
                    .take_while(|word| *word != "--")
                    .any(is_short_option_cluster_with_a)
        })
}

/// `true` for `-a`, `-ea`, `+a` and similar: one `-` or `+`, then letters
/// that include `a`.
fn is_short_option_cluster_with_a(word: &str) -> bool {
    word.strip_prefix(['-', '+'])
        .is_some_and(|flags| !flags.starts_with(['-', '+']) && flags.contains('a'))
}
