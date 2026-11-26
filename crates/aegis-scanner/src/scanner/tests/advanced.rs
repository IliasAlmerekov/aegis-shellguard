use super::*;

#[test]
fn custom_pattern_changes_assessment_and_marks_custom_source() {
    let custom = Pattern {
        id: "USR-ASS-001".into(),
        category: Category::Process,
        risk: RiskLevel::Danger,
        pattern: "deploy-prod-now".into(),
        description: "Project-specific destructive deploy shortcut".into(),
        safe_alt: Some("deploy-prod-now --dry-run".into()),
        justification: None,
        source: PatternSource::Custom,
    };

    let patterns =
        PatternSet::from_sources(&[custom]).expect("merged builtin+custom set should load");
    let scanner = Scanner::try_new(patterns).expect("custom pattern compiles");

    let assessment = scanner.assess("echo ok && deploy-prod-now");
    assert_eq!(assessment.risk, RiskLevel::Danger);
    assert_eq!(assessment.decision_source(), DecisionSource::CustomPattern);
    assert!(
        assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "USR-ASS-001"
                && m.pattern.source == PatternSource::Custom),
        "expected USR-ASS-001 custom match in assessment"
    );
}

#[test]
fn custom_pattern_with_invalid_regex_returns_typed_error_not_panic() {
    let bad = Pattern {
        id: "USR-BAD-REGEX".into(),
        category: Category::Process,
        risk: RiskLevel::Danger,
        pattern: "[".into(),
        description: "Pattern with a malformed regex".into(),
        safe_alt: None,
        justification: None,
        source: PatternSource::Custom,
    };

    let patterns = PatternSet::from_sources(&[bad])
        .expect("field validation passes; regex is not checked here");
    let result = Scanner::try_new(patterns);
    assert!(
        matches!(
            result,
            Err(crate::ScannerError::InvalidPattern { ref id, .. }) if id == "USR-BAD-REGEX"
        ),
        "expected InvalidPattern for USR-BAD-REGEX (got a different result)"
    );
}

#[test]
fn assess_danger_has_matched_patterns() {
    let s = scanner();
    let a = s.assess("rm -rf /home/user/project");
    assert!(
        !a.matched.is_empty(),
        "expected at least one matched pattern"
    );
    assert_eq!(a.risk, RiskLevel::Danger);
}

#[test]
fn assess_block_beats_danger() {
    let s = scanner();
    let a = s.assess("rm -rf /");
    assert_eq!(a.risk, RiskLevel::Block);
    let ids: Vec<&str> = a.matched.iter().map(|m| m.pattern.id.as_ref()).collect();
    assert!(
        ids.contains(&"PS-006"),
        "PS-006 must be in matched: {ids:?}"
    );
}

#[test]
fn assess_rm_root_delete_variants_with_split_and_extra_flags_as_block() {
    let s = scanner();

    let cases = [
        "rm -r -f /",
        "rm -R -f /",
        "rm -r --force /",
        "rm --recursive -f /",
        "rm -r --one-file-system -f /",
        "rm -rf --no-preserve-root /",
        "sudo rm -rf --no-preserve-root /",
    ];

    for cmd in cases {
        let assessment = s.assess(cmd);
        assert_eq!(
            assessment.risk,
            RiskLevel::Block,
            "command {cmd:?}: got {:?}, expected Block",
            assessment.risk,
        );
        assert!(
            assessment
                .matched
                .iter()
                .any(|m| m.pattern.id.as_ref() == "PS-006"),
            "command {cmd:?}: PS-006 must be in matched patterns: {:?}",
            assessment
                .matched
                .iter()
                .map(|m| m.pattern.id.as_ref())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn assess_rm_recursive_force_split_flags_on_non_root_paths_as_danger() {
    let s = scanner();

    let cases = [
        "rm -r -f /home/user/old-project",
        "rm -R -f /tmp/build",
        "rm --recursive --force /tmp/build",
        "rm -r --one-file-system -f /tmp/build",
    ];

    for cmd in cases {
        let assessment = s.assess(cmd);
        assert_eq!(
            assessment.risk,
            RiskLevel::Danger,
            "command {cmd:?}: got {:?}, expected Danger",
            assessment.risk,
        );
        assert!(
            assessment
                .matched
                .iter()
                .any(|m| m.pattern.id.as_ref() == "FS-001"),
            "command {cmd:?}: FS-001 must be in matched patterns: {:?}",
            assessment
                .matched
                .iter()
                .map(|m| m.pattern.id.as_ref())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn assess_does_not_match_rm_patterns_inside_longer_words() {
    let s = scanner();
    for cmd in ["echo farm -rf /", "echo farm -rf --no-preserve-root /"] {
        let assessment = s.assess(cmd);
        assert_eq!(
            assessment.risk,
            RiskLevel::Safe,
            "command {cmd:?}: got {:?}, expected Safe",
            assessment.risk,
        );
        assert!(
            assessment.matched.is_empty(),
            "command {cmd:?}: expected no matches, got {:?}",
            assessment
                .matched
                .iter()
                .map(|m| m.pattern.id.as_ref())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn assess_preserves_raw_command() {
    let s = scanner();
    let cmd = "git reset --hard HEAD~1";
    let a = s.assess(cmd);
    assert_eq!(a.command.raw, cmd);
}

#[test]
fn assess_compound_commands() {
    let s = scanner();

    let cases: &[(&str, RiskLevel)] = &[
        ("echo ok && rm -rf /home/user/project", RiskLevel::Danger),
        ("echo ok && rm -rf /", RiskLevel::Block),
        ("rm -rf /;echo done", RiskLevel::Block),
        ("rm -rf /&&echo done", RiskLevel::Block),
        ("rm -rf / && echo done", RiskLevel::Block),
        ("echo a; DROP TABLE users; echo b", RiskLevel::Danger),
        ("echo a; echo b; rm -rf /", RiskLevel::Block),
        (
            "echo creds | aws ec2 terminate-instances --instance-ids i-1234",
            RiskLevel::Danger,
        ),
        (
            "false || terraform destroy -auto-approve",
            RiskLevel::Danger,
        ),
        ("echo hello && ls /tmp && pwd", RiskLevel::Safe),
    ];

    for (cmd, expected) in cases {
        let assessment = s.assess(cmd);
        assert_eq!(
            assessment.risk, *expected,
            "compound command {cmd:?}: got {:?}, expected {expected:?}",
            assessment.risk,
        );
    }
}

#[test]
fn assess_bypass_prone_forms() {
    let s = scanner();

    let cases: &[(&str, RiskLevel)] = &[
        ("sh -c 'rm -rf /'", RiskLevel::Block),
        ("bash -c 'DROP TABLE users;'", RiskLevel::Danger),
        ("/bin/bash -c 'echo hello'", RiskLevel::Warn),
        ("bash -lc 'rm -rf /'", RiskLevel::Block),
        (
            "bash -ic 'terraform destroy -auto-approve'",
            RiskLevel::Danger,
        ),
        (
            "bash --login -c 'kubectl delete namespace production'",
            RiskLevel::Danger,
        ),
        (
            "MY_VAR=x bash -c 'aws ec2 terminate-instances --instance-ids i-1234'",
            RiskLevel::Danger,
        ),
        ("bash <<EOF\nrm -rf /\nEOF", RiskLevel::Block),
        (
            "bash <<EOF\nrm -rf /home/user/project\nEOF",
            RiskLevel::Danger,
        ),
        ("echo safe | bash -c 'rm -rf /'", RiskLevel::Block),
        ("echo ok; terraform destroy", RiskLevel::Danger),
        ("ls && DROP TABLE users;", RiskLevel::Danger),
        (
            "false || kubectl delete namespace staging",
            RiskLevel::Danger,
        ),
        ("echo $(rm -rf /)", RiskLevel::Block),
        ("(rm -rf /)", RiskLevel::Block),
        (
            r#"python3 -c "import os; os.system('rm -rf /')""#,
            RiskLevel::Danger,
        ),
        (r#"bash -c "rm -rf / && echo done""#, RiskLevel::Block),
    ];

    for (cmd, expected) in cases {
        let assessment = s.assess(cmd);
        assert_eq!(
            assessment.risk, *expected,
            "bypass form {cmd:?}: got {:?}, expected {expected:?}",
            assessment.risk,
        );
    }
}

#[test]
fn assess_indirect_execution_forms() {
    let s = scanner();

    let cases: &[(&str, RiskLevel)] = &[
        ("echo 'ls /tmp' | sh", RiskLevel::Danger),
        ("echo malicious_payload | bash", RiskLevel::Danger),
        ("bash -c 'echo hello'", RiskLevel::Warn),
        ("zsh -c 'echo hello'", RiskLevel::Warn),
        ("python -c 'import sys'", RiskLevel::Warn),
        ("python3 -c \"print('hi')\"", RiskLevel::Warn),
        ("python2 -ic \"import os\"", RiskLevel::Warn),
        ("python3 - <<'PY'\nprint('hi')\nPY", RiskLevel::Warn),
        ("node -e 'console.log(1)'", RiskLevel::Warn),
        ("nodejs -e 'process.version'", RiskLevel::Warn),
        ("perl -e 'print 42'", RiskLevel::Warn),
        ("eval \"printf hi\"", RiskLevel::Warn),
        ("eval \"$DEPLOY_CMD\"", RiskLevel::Warn),
        ("eval $INIT_SCRIPT", RiskLevel::Warn),
        ("eval \"${MY_BOOTSTRAP_SCRIPT}\"", RiskLevel::Warn),
        ("ruby -e 'puts 42'", RiskLevel::Warn),
        ("php -r 'echo 42;'", RiskLevel::Warn),
        ("lua -e 'print(42)'", RiskLevel::Warn),
        ("sh <(generate_config.sh)", RiskLevel::Warn),
        ("bash <(cat bootstrap.sh)", RiskLevel::Warn),
        ("source <(kubectl completion bash)", RiskLevel::Warn),
        ("source <(helm completion zsh)", RiskLevel::Warn),
    ];

    for (cmd, expected) in cases {
        let assessment = s.assess(cmd);
        assert_eq!(
            assessment.risk, *expected,
            "indirect execution form {cmd:?}: got {:?}, expected {expected:?}",
            assessment.risk,
        );
    }
}

#[test]
fn indirect_execution_safe_commands_not_flagged() {
    let s = scanner();

    for cmd in [
        "python3 script.py",
        "node server.js",
        "perl script.pl",
        "echo hello world",
        "echo hello | grep foo",
        "source ~/.bashrc",
        ". ~/.profile",
        "printf 'eval is just text'",
        "echo bash -c is documented here",
    ] {
        let assessment = s.assess(cmd);
        assert_eq!(
            assessment.risk,
            RiskLevel::Safe,
            "expected Safe for {cmd:?}, got {:?}",
            assessment.risk,
        );
    }
}

#[test]
fn indirect_execution_dangerous_body_escalates_risk() {
    let s = scanner();

    let cases: &[(&str, RiskLevel)] = &[
        (
            "python3 -c \"import os; os.system('shred -u secrets.key')\"",
            RiskLevel::Danger,
        ),
        (
            "node -e \"require('cp').execSync('terraform destroy')\"",
            RiskLevel::Warn,
        ),
        (
            "perl -e 'system(\"rm -rf /home/user/project\")'",
            RiskLevel::Danger,
        ),
        ("echo 'terraform destroy' | sh", RiskLevel::Danger),
    ];

    for (cmd, expected) in cases {
        let assessment = s.assess(cmd);
        assert_eq!(
            assessment.risk, *expected,
            "dangerous body escalation {cmd:?}: got {:?}, expected {expected:?}",
            assessment.risk,
        );
    }
}

#[test]
fn nested_execution_recursive_payloads_escalate_to_inner_risk() {
    let s = scanner();

    let cases: &[(&str, RiskLevel)] = &[
        (r#"source <(bash -c 'rm -rf /')"#, RiskLevel::Block),
        (r#"eval "bash -c 'rm -rf /'""#, RiskLevel::Block),
        ("bash <<'EOF'\nbash -c 'rm -rf /'\nEOF", RiskLevel::Block),
        (
            r#"bash -c 'source <(bash -c "rm -rf /")'"#,
            RiskLevel::Block,
        ),
    ];

    for (cmd, expected) in cases {
        let assessment = s.assess(cmd);
        assert_eq!(
            assessment.risk, *expected,
            "recursive nested payload {cmd:?}: got {:?}, expected {expected:?}",
            assessment.risk,
        );
    }
}

fn custom_with_regex(regex: String) -> Pattern {
    Pattern {
        id: "USR-LEN".into(),
        category: Category::Process,
        risk: RiskLevel::Danger,
        pattern: regex.into(),
        description: "Pattern length limit probe".into(),
        safe_alt: None,
        justification: None,
        source: PatternSource::Custom,
    }
}

fn try_new_error(regex: String) -> Option<String> {
    let patterns = PatternSet::from_sources(&[custom_with_regex(regex)]).expect("fields valid");
    Scanner::try_new(patterns).err().map(|e| e.to_string())
}

#[test]
fn custom_pattern_of_256_bytes_is_accepted() {
    assert_eq!(try_new_error("a".repeat(256)), None);
}

#[test]
fn custom_pattern_of_257_bytes_is_rejected_naming_length_and_limit() {
    let message = try_new_error("a".repeat(257)).expect("must be rejected");
    assert!(message.contains("USR-LEN"), "{message}");
    assert!(message.contains("257 bytes"), "{message}");
    assert!(message.contains("256"), "{message}");
}

#[test]
fn custom_pattern_length_is_counted_in_utf8_bytes_not_characters() {
    let message = try_new_error("é".repeat(129)).expect("must be rejected");
    assert!(message.contains("258 bytes"), "{message}");
}

#[test]
fn custom_pattern_over_the_compile_budget_fails_with_an_error_not_a_panic() {
    let regex = r"((\pL{100}){100}){100}".to_string();
    assert!(regex.len() < 256);
    let message = try_new_error(regex).expect("must be rejected");
    assert!(message.contains("invalid regex"), "{message}");
}
