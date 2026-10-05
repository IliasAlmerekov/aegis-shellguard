use super::*;

// #425: a token-prefix rule matches within one simple command. An `any_star()`
// wildcard must not run past `;`, `&&`, `||`, `|` or a newline into the flags
// of the next command.

#[test]
fn assess_flag_of_a_later_command_does_not_trip_git_push() {
    let s = scanner();
    for cmd in [
        "git push; gh api repos/o/r/issues/1/comments -f body=x",
        "git push && gh issue comment 1 -f",
        "git push || gh issue comment 1 -f",
        "git push | gh api x -f body=x",
        "git push\ngh api x -f body=x",
    ] {
        assert_eq!(s.assess(cmd).risk, RiskLevel::Safe, "command {cmd:?}");
    }
}

#[test]
fn assess_real_force_flag_in_any_command_of_the_chain_still_warns() {
    for cmd in [
        "git push -f && echo done",
        "echo start; git push origin main --force",
        "git push; git push -f",
        "git push\ngit push --force",
    ] {
        assert_assessment_matches_pattern(cmd, RiskLevel::Warn, "GIT-003");
    }
}
