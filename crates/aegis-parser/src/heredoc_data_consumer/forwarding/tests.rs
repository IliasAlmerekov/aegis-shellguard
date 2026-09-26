//! Unit tests for the forwarding-only allowlist (issue #396, ADR-042 item
//! 5). Split out of [`super`] to keep `forwarding.rs` under the 800-line
//! budget in `tests/file_size_budget.rs`.

use super::{
    bare_dot_command, find_variable_references, following_text_has_indirection,
    gh_subcommand_words, git_message_flag_trusted, is_whole_reference,
    segment_has_disallowed_redirect, segment_is_forwarding_only, split_top_level_chains,
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

// ── Issue #396 review follow-up: `printf -v`/`--` and `gh` subcommand
// gating (BLOCKER/MAJOR findings on PR #463) ────────────────────────────

#[test]
fn segment_is_forwarding_only_false_for_printf_dash_v() {
    // `-v` writes the value into a second variable instead of printing it;
    // the old index check only special-cased index 0 and missed this.
    assert!(!segment_is_forwarding_only(
        "printf -v y \"$x\"",
        "x",
        false
    ));
}

#[test]
fn segment_is_forwarding_only_false_for_printf_dash_dash() {
    // `--` shifts the format string to index 1, so a reference there is
    // the format string, not a forwarded argument.
    assert!(!segment_is_forwarding_only("printf -- \"$x\"", "x", false));
}

#[test]
fn segment_is_forwarding_only_true_for_gh_pr_create_message_flag() {
    assert!(segment_is_forwarding_only(
        "gh pr create --body \"$x\"",
        "x",
        false
    ));
}

#[test]
fn segment_is_forwarding_only_true_for_gh_issue_comment_message_flag() {
    assert!(segment_is_forwarding_only(
        "gh issue comment 1 -b \"$x\"",
        "x",
        false
    ));
}

#[test]
fn segment_is_forwarding_only_false_for_gh_pr_checkout_message_flag() {
    // `-b` names a branch for `pr checkout`, not a message.
    assert!(!segment_is_forwarding_only(
        "gh pr checkout 1 -b \"$x\"",
        "x",
        false
    ));
}

#[test]
fn segment_is_forwarding_only_false_for_gh_raw_field_outside_api() {
    // `-f` takes a `key=value` pair only under `gh api`.
    assert!(!segment_is_forwarding_only(
        "gh workflow run w -f a=\"$x\"",
        "x",
        false
    ));
}

#[test]
fn segment_is_forwarding_only_false_for_gh_repo_create_unlisted_flag() {
    assert!(!segment_is_forwarding_only(
        "gh repo create -d \"$x\"",
        "x",
        false
    ));
}

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

// Issue #396 review (finding 1): `git`'s subcommand gate applies to the
// forwarding-only allowlist too, the same gate `is_trusted_message_value`
// uses for a heredoc as the flag's whole value.
#[test]
fn segment_is_forwarding_only_false_for_git_commit_title_flag() {
    assert!(!segment_is_forwarding_only(
        "git commit -t \"$x\"",
        "x",
        false
    ));
}

#[test]
fn segment_is_forwarding_only_false_for_git_checkout_branch_flag() {
    assert!(!segment_is_forwarding_only(
        "git checkout -b \"$x\"",
        "x",
        false
    ));
}

#[test]
fn segment_is_forwarding_only_false_for_git_with_a_global_option_before_commit() {
    assert!(!segment_is_forwarding_only(
        "git -c alias.x=y commit -m \"$x\"",
        "x",
        false
    ));
}
