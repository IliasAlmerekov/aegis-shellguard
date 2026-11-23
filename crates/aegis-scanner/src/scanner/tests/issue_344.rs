use super::*;

// Quoted-delimiter heredoc (`<<'EOF'`) to a passive consumer disables shell
// expansion, so prose backticks inside the body stay Safe (#344).
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

// Unquoted heredocs expand backticks/`$(...)`, so they must fire.
#[test]
fn assess_unquoted_heredoc_backtick_command_substitution_still_fires() {
    assert_assessment_matches_pattern("cat <<EOF\n`rm -rf /`\nEOF", RiskLevel::Block, "FS-001");
}

// Interpreters execute stdin regardless of nowdoc quoting.
#[test]
fn assess_nowdoc_to_interpreter_still_scans_body() {
    assert_assessment_matches_pattern("bash <<'EOF'\nrm -rf /\nEOF", RiskLevel::Block, "FS-001");
}

// Unpiped `cat` is a trusted data consumer (ADR-042, #396).
#[test]
fn assess_nowdoc_to_cat_direct_dangerous_text_stays_safe() {
    let s = scanner();
    let assessment = s.assess("cat <<'EOF'\nrm -rf /\nEOF");

    assert_eq!(
        assessment.risk,
        RiskLevel::Safe,
        "expected Safe for a nowdoc body handed to cat, got {:?} ({:?})",
        assessment.risk,
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}
