//! The `git` and `gh` message-flag gates behind
//! [`super::is_trusted_message_value`] (issue #396): which flag, under which
//! subcommand, stores its value as a commit, tag, issue or PR message and
//! nothing else. Split out of [`super`] to keep it under the 800-line budget
//! in `tests/file_size_budget.rs`.

#[cfg(test)]
mod tests;

/// `git` flags whose value is only ever a commit, tag, merge or notes
/// message. `-t`/`--title` and `-b`/`--body` are deliberately not here: for
/// `git`, `-t` is `--template=<file>` (a file `git` reads) and `-b` names a
/// branch, neither a message (issue #396 review). Trusted only under the
/// subcommands [`GIT_MESSAGE_SUBCOMMANDS`] names.
const GIT_MESSAGE_FLAGS: &[&str] = &["-m", "--message"];

/// `git` subcommands whose `-m`/`--message` value ([`GIT_MESSAGE_FLAGS`]) is
/// stored as text: a commit, tag, merge or notes message. `stash` is left
/// out on purpose: `stash push -m`/`stash save` also take a message, but
/// telling that shape apart from `stash`'s many other forms needs more care
/// than this gate spends, so a stash message stays untrusted rather than
/// risk trusting the wrong one.
const GIT_MESSAGE_SUBCOMMANDS: &[&str] = &["commit", "tag", "merge", "notes"];

/// `true` when `flag` is a [`GIT_MESSAGE_FLAGS`] member and `args` (the
/// tokens right after `git` — a whole argument list, or just the words
/// before a later flag, since only `args[0]` matters here) resolves a
/// [`GIT_MESSAGE_SUBCOMMANDS`] member at position 0. A global option ahead
/// of the subcommand (`git -c alias.x=y commit -m`) occupies that position
/// instead, so it reads as "no subcommand" and the call is untrusted
/// (issue #396 review).
pub(super) fn git_message_flag_trusted(args: &[String], flag: &str) -> bool {
    if !GIT_MESSAGE_FLAGS.contains(&flag) {
        return false;
    }
    matches!(
        args.first(),
        Some(word) if !word.starts_with('-') && GIT_MESSAGE_SUBCOMMANDS.contains(&word.as_str())
    )
}

/// `gh` flags whose value is a message: a commit or tag message, an issue
/// or PR title or body. Trusted only under the
/// `gh` subcommands [`gh_message_flags_trusted`] names: `-b` means something
/// else entirely under, say, `gh pr checkout` (a branch, not a message).
const GH_MESSAGE_FLAGS: &[&str] = &["-m", "--message", "-t", "--title", "-b", "--body"];

/// `gh pr` actions whose message flag ([`GH_MESSAGE_FLAGS`]) value is stored
/// or sent as text. `pr checkout`, `pr list`, `pr diff` and the rest are not
/// on this list, since none of them reads `-b`/`-t`/`-m` as a message.
const GH_PR_MESSAGE_ACTIONS: &[&str] = &["create", "edit", "comment", "review", "merge"];

/// `gh issue` actions with the same property as [`GH_PR_MESSAGE_ACTIONS`],
/// for `gh issue`.
const GH_ISSUE_MESSAGE_ACTIONS: &[&str] = &["create", "edit", "comment"];

/// `gh`'s subcommand and, where it has one, its action (`pr`, `create`;
/// `issue`, `comment`) — read from the two fixed positions right after `gh`,
/// `args[0]` and `args[1]`, not the first two non-flag words found anywhere
/// in `args`. A flag occupying either position makes that slot (and, for a
/// flag at position 0, both slots) `None`: a value-taking flag before the
/// subcommand shifts every later word by however many tokens its own value
/// takes, and a fixed-position reader has no way to tell a flag's value
/// apart from the next subcommand word, so it must not guess (issue #396
/// review: `gh issue -R create delete --body "$x"` used to resolve to
/// `issue create` — `-R`'s own value, standing in for the real action word
/// `delete` — and `gh -R o/r pr create` used to resolve to `pr create` by
/// skipping `-R` and its value outright). [`gh_message_flags_trusted`]
/// reads an unresolved slot as untrusted, matching the rule that a subcommand this predicate cannot read falls on
/// the untrusted side — accepting `gh -R o/r pr create` as a false positive
/// rather than resolve it wrong.
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

/// `true` when `gh`'s resolved `(primary, secondary)` subcommand words
/// ([`gh_subcommand_words`]) are `pr` with a [`GH_PR_MESSAGE_ACTIONS`]
/// member, or `issue` with a [`GH_ISSUE_MESSAGE_ACTIONS`] member — the only
/// shapes where `-b`/`-t`/`-m` name a message rather than something `gh`
/// reads differently, such as `pr checkout`'s branch argument.
fn gh_message_flags_trusted(primary: Option<&str>, secondary: Option<&str>) -> bool {
    match primary {
        Some("pr") => secondary.is_some_and(|action| GH_PR_MESSAGE_ACTIONS.contains(&action)),
        Some("issue") => secondary.is_some_and(|action| GH_ISSUE_MESSAGE_ACTIONS.contains(&action)),
        _ => false,
    }
}

/// `true` when `flag` is a [`GH_MESSAGE_FLAGS`] member and `args` (the
/// tokens right after `gh` — a whole argument list, or just the words
/// before a later flag, since [`gh_subcommand_words`] only ever looks at
/// the first two) resolve to a subcommand [`gh_message_flags_trusted`]
/// approves. [`git_message_flag_trusted`]'s sibling for `gh` (issue #396
/// review).
pub(super) fn gh_message_flag_trusted(args: &[String], flag: &str) -> bool {
    if !GH_MESSAGE_FLAGS.contains(&flag) {
        return false;
    }
    let (primary, secondary) = gh_subcommand_words(args);
    gh_message_flags_trusted(primary, secondary)
}
