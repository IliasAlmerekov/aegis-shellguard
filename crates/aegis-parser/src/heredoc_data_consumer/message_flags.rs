//! `git` and `gh` message-flag gates behind [`super::is_trusted_message_value`] (#396).

#[cfg(test)]
mod tests;

/// `git` flags whose value is a message; `-t` (`--template`) and `-b` (branch) are excluded (#396).
const GIT_MESSAGE_FLAGS: &[&str] = &["-m", "--message"];

/// `git` subcommands whose `-m`/`--message` value is stored as text (`stash` excluded).
const GIT_MESSAGE_SUBCOMMANDS: &[&str] = &["commit", "tag", "merge", "notes"];

/// `true` when `flag` is in [`GIT_MESSAGE_FLAGS`] and `args[0]` is in [`GIT_MESSAGE_SUBCOMMANDS`].
///
/// A leading global option occupies `args[0]` and leaves the subcommand unresolved (#396).
pub(super) fn git_message_flag_trusted(args: &[String], flag: &str) -> bool {
    if !GIT_MESSAGE_FLAGS.contains(&flag) {
        return false;
    }
    matches!(
        args.first(),
        Some(word) if !word.starts_with('-') && GIT_MESSAGE_SUBCOMMANDS.contains(&word.as_str())
    )
}

/// `gh` flags whose value is a message under subcommands approved by [`gh_message_flags_trusted`].
const GH_MESSAGE_FLAGS: &[&str] = &["-m", "--message", "-t", "--title", "-b", "--body"];

/// `gh pr` actions where [`GH_MESSAGE_FLAGS`] name a message.
const GH_PR_MESSAGE_ACTIONS: &[&str] = &["create", "edit", "comment", "review", "merge"];

/// `gh issue` actions where [`GH_MESSAGE_FLAGS`] name a message.
const GH_ISSUE_MESSAGE_ACTIONS: &[&str] = &["create", "edit", "comment"];

/// Resolve `gh`'s `(primary, secondary)` subcommand words from `args[0]` and `args[1]`.
///
/// A flag in either position makes that slot (and any following slot) `None` so callers do not guess (#396).
fn gh_subcommand_words(args: &[String]) -> (Option<&str>, Option<&str>) {
    let is_word = |token: &String| !token.starts_with('-');
    let primary = args
        .first()
        .filter(|token| is_word(token))
        .map(String::as_str);
    let secondary = primary.and(
        args.get(1)
            .filter(|token| is_word(token))
            .map(String::as_str),
    );
    (primary, secondary)
}

/// `true` when `(primary, secondary)` is a `gh pr` or `gh issue` message action.
fn gh_message_flags_trusted(primary: Option<&str>, secondary: Option<&str>) -> bool {
    match primary {
        Some("pr") => secondary.is_some_and(|action| GH_PR_MESSAGE_ACTIONS.contains(&action)),
        Some("issue") => secondary.is_some_and(|action| GH_ISSUE_MESSAGE_ACTIONS.contains(&action)),
        _ => false,
    }
}

/// `true` when `flag` is in [`GH_MESSAGE_FLAGS`] and `args` resolve to a trusted `gh` message action (#396).
pub(super) fn gh_message_flag_trusted(args: &[String], flag: &str) -> bool {
    if !GH_MESSAGE_FLAGS.contains(&flag) {
        return false;
    }
    let (primary, secondary) = gh_subcommand_words(args);
    gh_message_flags_trusted(primary, secondary)
}
