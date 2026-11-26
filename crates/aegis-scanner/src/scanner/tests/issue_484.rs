use super::*;

#[test]
fn assess_flag_only_inside_quoted_argument_stays_safe() {
    let s = scanner();
    for cmd in [
        "git push origin 'x --force'",
        "git push origin \"x --force\"",
        "git clean -n 'docs -f'",
    ] {
        assert_eq!(s.assess(cmd).risk, RiskLevel::Safe, "command {cmd:?}");
    }
}

#[test]
fn assess_flag_after_end_of_options_stays_safe() {
    assert_eq!(
        scanner().assess("git push origin -- --force").risk,
        RiskLevel::Safe
    );
}

#[test]
fn assess_same_normalized_segment_with_a_real_flag_warns() {
    assert_assessment_matches_pattern(
        "git push 'x --force'; git push x --force",
        RiskLevel::Warn,
        "GIT-003",
    );
}

#[test]
fn assess_later_quoted_segment_keeps_earlier_segment_match() {
    assert_assessment_matches_pattern(
        "\"terraform destroy *\",\n\"docker system prune\",",
        RiskLevel::Danger,
        "CL-001",
    );
}

#[test]
fn assess_flag_inside_quoted_argument_behind_launcher_stays_safe() {
    assert_eq!(
        scanner().assess("sudo git push origin 'x --force'").risk,
        RiskLevel::Safe
    );
}

#[test]
fn assess_quoted_or_end_of_options_forms_that_still_warn() {
    for (cmd, risk, id) in [
        ("git push origin '--force'", RiskLevel::Warn, "GIT-003"),
        ("git -C 'my dir' push --force", RiskLevel::Warn, "GIT-003"),
        (
            "git -c alias.x='!git push --force' x",
            RiskLevel::Warn,
            "GIT-003",
        ),
        ("git clean -fd -- src", RiskLevel::Warn, "GIT-002"),
        ("rm -rf -- /", RiskLevel::Block, "PS-006"),
    ] {
        assert_assessment_matches_pattern(cmd, risk, id);
    }
}

#[test]
fn assess_real_flag_after_a_quoted_double_dash_operand_still_warns() {
    for (cmd, id) in [
        ("true; git push origin 'a --' --force", "GIT-003"),
        ("echo hi | git push origin 'a --' -f", "GIT-003"),
        ("echo hi && git push origin \"a --\" --force", "GIT-003"),
        (
            "cd /repo && git push origin 'refs/heads/x --' --force",
            "GIT-003",
        ),
        ("true; git push origin a\\ -- --force", "GIT-003"),
        ("true; git clean 'a --' -fdx", "GIT-002"),
        ("true; rsync -a 'a --' --delete src/ dst/", "FS-015"),
        ("true; FOO=1 git push origin 'a --' --force", "GIT-003"),
    ] {
        let assessment = scanner().assess(cmd);
        assert_eq!(assessment.risk, RiskLevel::Warn, "command {cmd:?}");
        assert!(
            assessment
                .matched
                .iter()
                .any(|m| m.pattern.id.as_ref() == id),
            "command {cmd:?} should match {id}"
        );
    }
}

#[test]
fn assess_flag_after_real_double_dash_in_later_segment_stays_safe() {
    for cmd in [
        "true; git push origin -- --force",
        "true; git push origin 'x --force'",
    ] {
        assert_eq!(
            scanner().assess(cmd).risk,
            RiskLevel::Safe,
            "command {cmd:?}"
        );
    }
}
