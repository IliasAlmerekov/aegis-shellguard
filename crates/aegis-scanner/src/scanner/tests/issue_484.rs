use super::*;

// #484: a flag that a token-prefix rule looks for after `any_star()` matched
// when it appeared only inside a quoted argument, or as an operand after `--`.

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

// Both segments normalize to `git push x --force`, but only the second one
// passes a standalone `--force`, so its tokens must not be swapped for the
// first segment's quoted ones.
#[test]
fn assess_same_normalized_segment_with_a_real_flag_warns() {
    assert_assessment_matches_pattern(
        "git push 'x --force'; git push x --force",
        RiskLevel::Warn,
        "GIT-003",
    );
}

// The filter runs once per scan target and must leave the matches of earlier
// targets alone: CL-001 from the first line survived the second line's pass.
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

// The filter only removes matches the quoted tokens contradict. A quoted flag
// is still a flag, a quoted git global option still shifts the subcommand, an
// alias body that names the program keeps its match, and `--` does not hide
// an operand rule such as PS-006.
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
