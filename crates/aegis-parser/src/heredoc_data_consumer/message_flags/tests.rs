use super::{gh_subcommand_words, git_message_flag_trusted};

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
