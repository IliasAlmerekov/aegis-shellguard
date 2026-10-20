use super::*;

// Bash decodes ANSI-C quoting (`$'...'`) before it runs a word, so a quoted
// spelling must classify exactly like the word it decodes to.

#[test]
fn assess_ansi_c_program_word_matches_the_decoded_program() {
    for cmd in [
        r"$'\162m' -rf /tmp/x",
        r"$'\x72m' -rf /tmp/x",
        r"$'rm' -rf /tmp/x",
        r"$'\U00000072m' -rf /tmp/x",
        r"r$'m' -rf /tmp/x",
        r"$'r''m' -rf /tmp/x",
        r#"$"rm" -rf /tmp/x"#,
    ] {
        assert_assessment_matches_pattern(cmd, RiskLevel::Danger, "FS-001");
    }
}

#[test]
fn assess_ansi_c_argument_matches_the_decoded_argument() {
    assert_assessment_matches_pattern(r"rm $'-rf' /tmp/x", RiskLevel::Danger, "FS-001");
    assert_assessment_matches_pattern(r"git $'push' --force", RiskLevel::Warn, "GIT-003");
    assert_assessment_matches_pattern(r"git push $'--force'", RiskLevel::Warn, "GIT-003");
}

#[test]
fn assess_ansi_c_word_in_nested_positions_matches_the_decoded_program() {
    for cmd in [
        r"echo $( $'\162m' -rf /tmp/x )",
        r"true && $'\162m' -rf /tmp/x",
        r"sudo $'\162m' -rf /tmp/x",
        r"env $'\162m' -rf /tmp/x",
        r"cd $'/tmp' && $'\162m' -rf x",
        r"bash -c $'\162m -rf /tmp/x'",
    ] {
        assert_assessment_matches_pattern(cmd, RiskLevel::Danger, "FS-001");
    }
}

#[test]
fn assess_escaped_quote_in_ansi_c_word_keeps_command_boundaries() {
    assert_assessment_matches_pattern(
        r"echo $'\'' ; git push --force ; echo $'\''",
        RiskLevel::Warn,
        "GIT-003",
    );
}

#[test]
fn assess_inexact_ansi_c_word_is_not_safe() {
    let s = scanner();
    for cmd in [
        r"$'\162m -rf /tmp/x",
        r"$'\xff' -rf /tmp/x",
        r"echo $'\U110000'",
    ] {
        let assessment = s.assess(cmd);
        assert!(
            assessment.risk >= RiskLevel::Warn,
            "command {cmd:?}: got {:?}",
            assessment.risk
        );
        assert!(assessment.effect_opaque, "command {cmd:?}");
    }
}

#[test]
fn assess_nul_in_ansi_c_word_reads_the_word_bash_runs() {
    assert_assessment_matches_pattern(r"$'r\0junk'm -rf /tmp/x", RiskLevel::Danger, "FS-001");
}

#[test]
fn assess_backslash_and_quote_spellings_match_the_plain_program() {
    for cmd in [
        r"\rm -rf /tmp/x",
        r"r\m -rf /tmp/x",
        "r''m -rf /tmp/x",
        r#""r"m -rf /tmp/x"#,
        r"rm $'\'' -rf / $'\''",
    ] {
        let assessment = scanner().assess(cmd);
        assert!(
            assessment.risk >= RiskLevel::Danger,
            "command {cmd:?}: got {:?}",
            assessment.risk
        );
    }
}

#[test]
fn assess_plain_ansi_c_data_stays_safe() {
    let s = scanner();
    for cmd in [r"echo $'hi\n'", r"printf $'a\tb\n'"] {
        assert_eq!(s.assess(cmd).risk, RiskLevel::Safe, "command {cmd:?}");
    }
}

#[test]
fn assess_keeps_the_raw_spelling_for_audit() {
    let cmd = r"$'\162m' -rf /tmp/x";
    assert_eq!(scanner().assess(cmd).command.raw, cmd);
}

#[test]
fn git_stage_report_finds_a_git_stage_after_an_escaped_ansi_c_quote() {
    let s = scanner();
    let cmd = r"echo $'\'' ; git -C $'oth\x65r' filter-branch --index-filter true ; echo $'\''";
    let assessment = s.assess(cmd);
    let report = s.git_stage_report_with(cmd, &assessment, crate::scanner::AmbientEnv::default());

    let targets: Vec<_> = report
        .redirected
        .iter()
        .map(|stage| match &stage.target {
            crate::scanner::DangerStageTarget::Options(readings) => readings
                .iter()
                .map(|args| args.dash_c.clone())
                .collect::<Vec<_>>(),
            other => panic!("expected options, got {other:?}"),
        })
        .collect();
    assert_eq!(targets, vec![vec![vec!["other".to_string()]]]);
}
