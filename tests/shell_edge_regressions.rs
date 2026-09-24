use aegis_scanner::assess;
use aegis_types::RiskLevel;

#[test]
fn regression_commands_with_heredoc_hermetically_block_or_warn() {
    let command = "bash <<'EOF'\necho 'prepare'\nrm -rf /tmp/aegis-fuzz-shell-regression\nEOF\n";

    let assessment = assess(command).expect("assessment should not fail");
    assert!(
        assessment.risk >= RiskLevel::Warn,
        "expected heredoc payload with rm to be risky: {command:?}"
    );
    assert!(
        assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "FS-001" || m.pattern.id.as_ref() == "SCAN-001"),
        "expected heredoc regression command to match a scanner pattern"
    );
}

#[test]
fn regression_commands_with_inline_python_script_are_flagged() {
    let command = r#"python3 -c "import os; os.system('rm -rf /tmp/aegis-inline-regression')""#;

    let assessment = assess(command).expect("assessment should not fail");
    assert!(
        assessment.risk >= RiskLevel::Warn,
        "expected inline script payload to be risky: {command:?}"
    );
    assert!(
        assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "PKG-001"
                || m.pattern.id.as_ref() == "FS-001"
                || m.pattern.id.as_ref() == "PS-010"),
        "expected inline script regression command to match inline-script or scanner pattern"
    );
}

#[test]
fn regression_commands_with_pipes_and_chains_are_covered() {
    let command = "cat /etc/passwd | bash -lc \"echo ok; rm -rf /tmp/aegis-pipe-regression\"";

    let assessment = assess(command).expect("assessment should not fail");
    assert!(
        assessment.risk >= RiskLevel::Warn,
        "expected piped chain regression command to be risky: {command:?}"
    );
    assert!(
        assessment.matched.iter().any(|m| !m.pattern.id.is_empty()),
        "expected piped chain command to produce at least one match"
    );
}

#[test]
fn regression_commands_with_quotes_and_escape_sequences_are_scanned() {
    let command = "echo \"safe token: rm -rf /tmp/aegis-quoted-regression\" && true";

    let assessment = assess(command).expect("assessment should not fail");
    assert!(
        assessment.risk >= RiskLevel::Warn,
        "expected quoted escape content to remain risky: {command:?}"
    );
    assert!(
        assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "FS-001"),
        "expected quoted regression command to match rm command pattern"
    );
}

#[test]
fn regression_commands_with_subshell_grouping_and_multiline_payloads() {
    let command =
        "(\n    echo start\n    rm -rf /tmp/aegis-subshell-regression\n) && { echo done; }\n";

    let assessment = assess(command).expect("assessment should not fail");
    assert!(
        assessment.risk >= RiskLevel::Warn,
        "expected subshell and multiline regression command to be risky: {command:?}"
    );
    assert!(
        assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "FS-001"),
        "expected subshell regression command to match scanner pattern"
    );
}

#[test]
fn regression_docker_compose_down_with_volume_flag_is_flagged() {
    let command = "docker compose down -v";

    let assessment = assess(command).expect("assessment should not fail");
    assert_eq!(
        assessment.risk,
        RiskLevel::Warn,
        "expected docker compose down -v to be warn: {command:?}"
    );
    assert!(
        assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "DK-003"),
        "expected docker compose down -v to match DK-003"
    );
}

// GHSA-7gcj-4f7x-7fxj / #415: FS-001's regex only fired when the recursive
// flag was the very first token after `rm`, so a flag trailing the operand
// went undetected end to end. FS-020 (a token-prefix rule) closes this gap.
#[test]
fn regression_rm_recursive_flag_after_operand_is_flagged() {
    let command = "rm /tmp/aegis-recursive-regression -r";

    let assessment = assess(command).expect("assessment should not fail");
    assert_eq!(
        assessment.risk,
        RiskLevel::Danger,
        "expected rm with a trailing -r flag to be Danger: {command:?}"
    );
    assert!(
        assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "FS-020"),
        "expected trailing-flag rm regression command to match FS-020"
    );
}

// Same gap, root case: PS-006's regex has the same first-token-only shape,
// so a recursive rm of `/` with a leading unrelated flag was Danger (via
// FS-020) instead of Block. PS-008 closes it.
#[test]
fn regression_rm_recursive_root_with_leading_flag_is_blocked() {
    let command = "rm -v -rf /";

    let assessment = assess(command).expect("assessment should not fail");
    assert_eq!(
        assessment.risk,
        RiskLevel::Block,
        "expected rm -v -rf / to be Block: {command:?}"
    );
    assert!(
        assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "PS-008"),
        "expected root regression command to match PS-008"
    );
}
