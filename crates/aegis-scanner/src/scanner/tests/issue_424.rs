use super::*;

// #424: `git commit --amend` rewrites the last commit, yet no built-in rule
// keyed on it, so it fell through to `Safe` like a plain `git commit`.

#[test]
fn assess_commit_amend_anywhere_in_arguments_warns_via_git010() {
    for cmd in [
        "git commit --amend --no-edit",
        "git commit --amend -m 'fix typo'",
        "git commit -q --amend --no-edit -a",
        "git commit --amend -m \"$(cat <<'EOF'\nfix typo\nEOF\n)\"",
        "git -C . commit --amend --no-edit",
    ] {
        assert_assessment_matches_pattern(cmd, RiskLevel::Warn, "GIT-010");
    }
}

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
fn assess_commit_without_amend_stays_safe() {
    let s = scanner();
    for cmd in [
        "git commit -m 'fix typo'",
        "git commit --no-edit",
        "git commit --all -m x",
    ] {
        assert_eq!(s.assess(cmd).risk, RiskLevel::Safe, "command {cmd:?}");
    }
}
