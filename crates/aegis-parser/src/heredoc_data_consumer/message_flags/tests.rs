//! Unit tests for the `git`/`gh` message-flag gates (#396).

use super::{gh_subcommand_words, git_message_flag_trusted};

// For `git`, `-t` is `--template` and `-b` names a branch, not a message (#396).
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

// A global option before the subcommand leaves the subcommand unresolved (#396).
#[test]
fn git_message_flag_trusted_false_when_a_global_option_precedes_the_subcommand() {
    let args = vec!["-c".to_owned(), "alias.x=y".to_owned(), "commit".to_owned()];
    assert!(!git_message_flag_trusted(&args, "-m"));
}

#[test]
fn git_message_flag_trusted_false_for_checkout_dash_b() {
    let args = vec!["checkout".to_owned()];
    assert!(!git_message_flag_trusted(&args, "-b"));
}

// A flag before the second word leaves the action slot `None` (#396).
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

// A flag before the first word leaves both slots `None` (#396).
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
