//! Whether the variable a `NAME=$(cat <<'EOF' ...)` capture assigns can run
//! without any later `$NAME` (PR #463 review). Git, bash, and editors read
//! some variables themselves: `GIT_SSH_COMMAND`, `GIT_EDITOR`, `PS4` under
//! `set -x`, `BASH_ENV` in a child bash. A denylist of such names would miss
//! the next one, so trust is limited to names with no uppercase letter,
//! which bash and common tools never read as code, and that never reach a
//! child's environment through an env prefix or an export. zsh is the one
//! exception: it ties a few lowercase names to uppercase ones
//! ([`ZSH_SPECIAL_NAMES`]).

/// Builtins that can export a variable or give it an attribute. Any of them
/// in the text around the capture keeps the body scanned, whatever flags
/// follow, since `declare -x` and `local -x` export as `export` does.
const EXPORT_WORDS: &[&str] = &["declare", "export", "local", "typeset"];

/// Lowercase names zsh reads itself. `path`, `fpath`, `cdpath`, `manpath`
/// and `module_path` are tied to their uppercase forms, so `path=...`
/// changes which program the next bare word runs (PR #463 review). The
/// hook arrays name functions zsh calls on its own, and `prompt` is `PS1`.
const ZSH_SPECIAL_NAMES: &[&str] = &[
    "cdpath",
    "chpwd_functions",
    "fpath",
    "manpath",
    "module_path",
    "path",
    "periodic_functions",
    "precmd_functions",
    "preexec_functions",
    "prompt",
    "zshaddhistory_functions",
    "zshexit_functions",
];

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
        || ZSH_SPECIAL_NAMES.contains(&name)
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
/// A `set` whose arguments hold a `$` or a backtick counts too, since an
/// expansion such as `set${IFS}-a` becomes `set -a` only after bash splits
/// it (PR #463 review).
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
    unquoted.match_indices("set").any(|(start, _)| {
        let is_word_char = |c: char| c.is_ascii_alphanumeric() || c == '_';
        let before = unquoted[..start].chars().next_back();
        let rest = &unquoted[start + "set".len()..];
        if before.is_some_and(is_word_char) || rest.starts_with(is_word_char) {
            return false;
        }
        let arguments = rest
            .split(['\n', ';', '&', '|', '(', ')'])
            .next()
            .unwrap_or_default();
        arguments.contains(['$', '`'])
            || arguments
                .split_whitespace()
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
