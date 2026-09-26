//! Unit tests for the forwarding-only allowlist (issue #396, ADR-042 item
//! 5). Split out of [`super`] to keep `forwarding.rs` under the 800-line
//! budget in `tests/file_size_budget.rs`.

use super::{
    bare_dot_command, find_variable_references, following_text_has_indirection, is_whole_reference,
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
