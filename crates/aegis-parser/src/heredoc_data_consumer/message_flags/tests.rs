//! Unit tests for the `git`/`gh` message-flag gates (issue #396). Split
//! out of [`super`] to keep `message_flags.rs` small.

use super::{gh_subcommand_words, git_message_flag_trusted};

// Issue #396 review (finding 1): `git`'s message flag is only ever `-m`, or
// `--message` — `-t`/`--title` and `-b`/`--body` mean something else for
// `git` (a template file, a branch), and must never be trusted regardless
// of subcommand.
#[test]
fn git_message_flag_trusted_false_for_title_flag_under_commit() {
    let args = vec!["commit".to_owned()];
    assert!(!git_message_flag_trusted(&args, "-t"));
}

#[test]
fn git_message_flag_trusted_false_for_body_flag_under_commit() {
    let args = vec!["commit".to_owned()];
    assert!(!git_message_flag_trusted(&args, "-b"));
}

#[test]
fn git_message_flag_trusted_true_for_dash_m_under_commit() {
    let args = vec!["commit".to_owned()];
    assert!(git_message_flag_trusted(&args, "-m"));
}

#[test]
fn git_message_flag_trusted_true_for_dash_m_under_tag() {
    let args = vec!["tag".to_owned(), "-a".to_owned(), "v1".to_owned()];
    assert!(git_message_flag_trusted(&args, "-m"));
}

// Issue #396 review (finding 1): a global option before the subcommand
// (`git -c alias.x=y commit -m`) occupies the position this gate reads as
// the subcommand, so it must read as "no subcommand" rather than skip past
// the option to `commit`.
#[test]
fn git_message_flag_trusted_false_when_a_global_option_precedes_the_subcommand() {
    let args = vec!["-c".to_owned(), "alias.x=y".to_owned(), "commit".to_owned()];
    assert!(!git_message_flag_trusted(&args, "-m"));
}

#[test]
fn git_message_flag_trusted_false_for_checkout_dash_b() {
    // `-b` names a branch for `git checkout`, not a message, and `checkout`
    // is not a message subcommand at all.
    let args = vec!["checkout".to_owned()];
    assert!(!git_message_flag_trusted(&args, "-b"));
}

// Issue #396 review (finding 3): a flag before the second word shifts what
// the resolver would otherwise read as the action, so that slot must read
// as unresolved rather than skip past the flag to the next word.
#[test]
fn gh_subcommand_words_none_secondary_when_a_flag_sits_between_the_two_words() {
    let args = vec![
        "issue".to_owned(),
        "-R".to_owned(),
        "create".to_owned(),
        "delete".to_owned(),
    ];
    assert_eq!(gh_subcommand_words(&args), (Some("issue"), None));
}

// Issue #396 review (finding 2 and 3): a flag before the first word makes
// both slots unresolved — never skipped past to reach `pr create` behind it.
#[test]
fn gh_subcommand_words_none_both_when_a_flag_sits_before_the_first_word() {
    let args = vec![
        "-R".to_owned(),
        "o/r".to_owned(),
        "pr".to_owned(),
        "create".to_owned(),
    ];
    assert_eq!(gh_subcommand_words(&args), (None, None));
}
