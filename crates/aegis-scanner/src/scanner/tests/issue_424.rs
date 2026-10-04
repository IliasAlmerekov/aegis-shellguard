use super::*;

// #424: `git commit --amend` rewrites the last commit, yet no built-in rule
// keyed on it, so it fell through to `Safe` like a plain `git commit`. GIT-010
// matches `--amend` anywhere in the `git commit` arguments.

#[test]
fn assess_commit_amend_no_edit_warns_via_git010() {
    assert_assessment_matches_pattern("git commit --amend --no-edit", RiskLevel::Warn, "GIT-010");
}

#[test]
fn assess_commit_amend_with_message_warns_via_git010() {
    assert_assessment_matches_pattern(
        "git commit --amend -m 'fix typo'",
        RiskLevel::Warn,
        "GIT-010",
    );
}

#[test]
fn assess_commit_amend_after_other_flags_warns_via_git010() {
    assert_assessment_matches_pattern(
        "git commit -q --amend --no-edit -a",
        RiskLevel::Warn,
        "GIT-010",
    );
}

#[test]
fn assess_commit_amend_with_heredoc_message_warns_via_git010() {
    assert_assessment_matches_pattern(
        "git commit --amend -m \"$(cat <<'EOF'\nfix typo\nEOF\n)\"",
        RiskLevel::Warn,
        "GIT-010",
    );
}

#[test]
fn assess_commit_amend_behind_git_global_option_warns_via_git010() {
    assert_assessment_matches_pattern(
        "git -C . commit --amend --no-edit",
        RiskLevel::Warn,
        "GIT-010",
    );
}

// git accepts any unambiguous prefix of a long option, and `--am` is already
// unambiguous among `git commit` options.
#[test]
fn assess_commit_abbreviated_amend_warns_via_git010() {
    for cmd in [
        "git commit --am --no-edit",
        "git commit --ame --no-edit",
        "git commit --amen --no-edit",
    ] {
        assert_assessment_matches_pattern(cmd, RiskLevel::Warn, "GIT-010");
    }
}

#[test]
fn assess_plain_commit_stays_safe() {
    assert_eq!(
        scanner().assess("git commit -m 'fix typo'").risk,
        RiskLevel::Safe
    );
}
