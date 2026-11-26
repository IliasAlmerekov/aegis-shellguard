#[cfg(test)]
mod tests;

const GIT_MESSAGE_FLAGS: &[&str] = &["-m", "--message"];

const GIT_MESSAGE_SUBCOMMANDS: &[&str] = &["commit", "tag", "merge", "notes"];

pub(super) fn git_message_flag_trusted(args: &[String], flag: &str) -> bool {
    if !GIT_MESSAGE_FLAGS.contains(&flag) {
        return false;
    }
    matches!(
        args.first(),
        Some(word) if !word.starts_with('-') && GIT_MESSAGE_SUBCOMMANDS.contains(&word.as_str())
    )
}

const GH_MESSAGE_FLAGS: &[&str] = &["-m", "--message", "-t", "--title", "-b", "--body"];

const GH_PR_MESSAGE_ACTIONS: &[&str] = &["create", "edit", "comment", "review", "merge"];

const GH_ISSUE_MESSAGE_ACTIONS: &[&str] = &["create", "edit", "comment"];

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

fn gh_message_flags_trusted(primary: Option<&str>, secondary: Option<&str>) -> bool {
    match primary {
        Some("pr") => secondary.is_some_and(|action| GH_PR_MESSAGE_ACTIONS.contains(&action)),
        Some("issue") => secondary.is_some_and(|action| GH_ISSUE_MESSAGE_ACTIONS.contains(&action)),
        _ => false,
    }
}

pub(super) fn gh_message_flag_trusted(args: &[String], flag: &str) -> bool {
    if !GH_MESSAGE_FLAGS.contains(&flag) {
        return false;
    }
    let (primary, secondary) = gh_subcommand_words(args);
    gh_message_flags_trusted(primary, secondary)
}
