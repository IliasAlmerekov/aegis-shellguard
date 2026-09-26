use super::*;

// Issue #357: a heredoc body handed to `cat`/`tee` whose own output is
// redirected into a file is pure data at rest — nothing in the pipeline ever
// runs or displays it. A dangerous-looking substring inside it (a Rust test
// fixture, a changelog entry) must not be treated as a live command.
#[test]
fn assess_nowdoc_redirected_into_file_with_dangerous_text_stays_safe() {
    let s = scanner();
    let cmd = "cat >> notes.txt <<'EOF'\nlet cmd = \"rm -rf .\";\nEOF";
    let assessment = s.assess(cmd);

    assert_eq!(
        assessment.risk,
        RiskLevel::Safe,
        "expected Safe for a nowdoc body written to a file, got {:?} ({:?})",
        assessment.risk,
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}

// Same shape with `>` instead of `>>`, and with `tee` instead of `cat`.
#[test]
fn assess_nowdoc_redirected_into_file_variants_stay_safe() {
    let cases = [
        "cat > notes.txt <<'EOF'\nrm -rf /\nEOF",
        "tee notes.txt <<'EOF'\nrm -rf /\nEOF",
        "tee -a notes.txt <<'EOF'\nrm -rf /\nEOF",
    ];

    let s = scanner();
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

// The exact repro from the issue: an unrelated command chained after the
// heredoc write must still be evaluated on its own merits — masking the
// heredoc body must not blind the scanner to a genuinely dangerous command
// elsewhere on the line.
#[test]
fn assess_redirected_heredoc_does_not_mask_a_trailing_dangerous_command() {
    assert_assessment_matches_pattern(
        "cat >> notes.txt <<'EOF'\nharmless log line\nEOF\nrm -rf /",
        RiskLevel::Block,
        "FS-001",
    );
}

// No output redirection at all: `cat <<'EOF' ... EOF` prints the body to the
// terminal. Issue #396 widens #357's reasoning: `cat` never executes its
// stdin, and the terminal is not a shell, so this is now a safe case rather
// than the #344 guardrail it used to be.
#[test]
fn assess_nowdoc_to_cat_without_redirection_is_now_safe() {
    let s = scanner();
    let assessment = s.assess("cat <<'EOF'\nrm -rf /\nEOF");

    assert_eq!(
        assessment.risk,
        RiskLevel::Safe,
        "expected Safe for a bare nowdoc body handed to cat, got {:?} ({:?})",
        assessment.risk,
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}

// A nowdoc handed to an interpreter, even with its own stdout redirected to
// a file, still gets executed as code by that interpreter before anything
// reaches the file — it must keep being scanned.
#[test]
fn assess_nowdoc_to_interpreter_with_redirection_still_fires() {
    assert_assessment_matches_pattern(
        "bash > log.txt <<'EOF'\nrm -rf /\nEOF",
        RiskLevel::Block,
        "FS-001",
    );
}

// `2>&1` duplicates stderr onto stdout — stdout still prints to the
// terminal, same destination as the bare case above, so this is safe for
// the same reason `assess_nowdoc_to_cat_without_redirection_is_now_safe` is
// (issue #396).
#[test]
fn assess_nowdoc_to_cat_with_fd_duplication_is_now_safe() {
    let s = scanner();
    let assessment = s.assess("cat 2>&1 <<'EOF'\nrm -rf /\nEOF");

    assert_eq!(
        assessment.risk,
        RiskLevel::Safe,
        "expected Safe for a fd-duplicated nowdoc body handed to cat, got {:?} ({:?})",
        assessment.risk,
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}

// An unquoted (non-nowdoc) heredoc still undergoes bash's own expansion
// while being constructed, redirection or not, so command substitution in
// its body must still be scanned.
#[test]
fn assess_unquoted_heredoc_redirected_into_file_backtick_substitution_still_fires() {
    assert_assessment_matches_pattern(
        "cat >> notes.txt <<EOF\n`rm -rf /`\nEOF",
        RiskLevel::Block,
        "FS-001",
    );
}
