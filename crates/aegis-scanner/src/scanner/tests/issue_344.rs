use super::*;

// Issue #344: a quoted-delimiter heredoc (`<<'EOF'`) disables all shell
// expansion in its body, including backtick command substitution. A
// dangerous-looking substring inside that body — a markdown code span in
// prose, say — is inert text, not a command about to run, as long as the
// heredoc target itself is a passive consumer of stdin rather than an
// interpreter. Scanning must not treat it as live command substitution.
#[test]
fn assess_quoted_delimiter_heredoc_prose_backticks_stay_safe() {
    let s = scanner();
    let cmd = "cat <<'EOF'\nmentions `python3 -c` as inert markdown code text\nEOF";
    let assessment = s.assess(cmd);

    assert_eq!(
        assessment.risk,
        RiskLevel::Safe,
        "expected Safe for prose backticks inside a nowdoc body, got {:?} ({:?})",
        assessment.risk,
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}

// The exact `gh pr create --body "$(cat <<'EOF' ... EOF)"` shape from the
// issue report.
#[test]
fn assess_gh_pr_body_heredoc_with_prose_backticks_stays_safe() {
    let s = scanner();
    let cmd = "gh pr create --body \"$(cat <<'EOF'\nmentions `python3 -c` as inert markdown code text, quoted heredoc terminator disables all shell expansion here\nEOF\n)\"";
    let assessment = s.assess(cmd);

    assert_eq!(
        assessment.risk,
        RiskLevel::Safe,
        "expected Safe, got {:?} ({:?})",
        assessment.risk,
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}

// Double-quoted and backslash-escaped delimiters are nowdoc too, per POSIX —
// same inert-text treatment applies.
#[test]
fn assess_double_quoted_and_backslash_delimiter_heredocs_stay_safe() {
    let s = scanner();
    let cases = [
        "cat <<\"EOF\"\nmentions `python3 -c` as inert markdown code text\nEOF",
        "cat <<\\EOF\nmentions `python3 -c` as inert markdown code text\nEOF",
    ];

    for cmd in cases {
        let assessment = s.assess(cmd);
        assert_eq!(
            assessment.risk,
            RiskLevel::Safe,
            "command {cmd:?}: expected Safe, got {:?}",
            assessment.risk
        );
    }
}

// An unquoted heredoc still expands backticks/`$(...)` at construction time —
// this must keep firing exactly as before the fix.
#[test]
fn assess_unquoted_heredoc_backtick_command_substitution_still_fires() {
    assert_assessment_matches_pattern("cat <<EOF\n`rm -rf /`\nEOF", RiskLevel::Block, "FS-001");
}

// A nowdoc handed to an interpreter (not a passive consumer) still executes
// its raw body as code once the interpreter reads it from stdin — nowdoc
// quoting only suppresses bash's own expansion, not the interpreter's.
#[test]
fn assess_nowdoc_to_interpreter_still_scans_body() {
    assert_assessment_matches_pattern("bash <<'EOF'\nrm -rf /\nEOF", RiskLevel::Block, "FS-001");
}

// A nowdoc body that is itself dangerous text (not hidden behind backticks)
// must still be caught — masking only neutralizes command-substitution
// markers, it does not exempt the body from direct pattern matching.
#[test]
fn assess_nowdoc_to_cat_direct_dangerous_command_still_fires() {
    assert_assessment_matches_pattern("cat <<'EOF'\nrm -rf /\nEOF", RiskLevel::Block, "FS-001");
}
