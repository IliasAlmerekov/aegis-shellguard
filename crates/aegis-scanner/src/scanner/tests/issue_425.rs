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

// #449: the same wildcard run made `FS-020` read `-rn` of a later `grep` as a
// flag of `rm`.

#[test]
fn assess_recursive_flag_of_a_later_command_does_not_trip_rm() {
    let s = scanner();
    for cmd in [
        "rm a && grep -rn x .",
        "rm a; grep -rn x .",
        "rm a || ls -R",
        "rm a && git diff -R",
        "rm a | grep -rn x .",
    ] {
        assert_eq!(s.assess(cmd).risk, RiskLevel::Safe, "command {cmd:?}");
    }
}

#[test]
fn assess_real_recursive_rm_in_a_chain_still_trips_fs_020() {
    assert_assessment_matches_pattern("echo go && rm -r build", RiskLevel::Danger, "FS-020");
}

// #449: `PS-008` shares `rm_recursive_flag_present` with `FS-020`, so a later
// command's `-r` and `/` must not read as `rm -r /`.

#[test]
fn assess_recursive_flag_and_root_of_a_later_command_does_not_trip_ps_008() {
    let s = scanner();
    for cmd in [
        "rm a && grep -rn x /",
        "rm a; grep -r x /",
        "rm a || ls -R /",
        "rm a | grep -rn x /",
    ] {
        assert_eq!(s.assess(cmd).risk, RiskLevel::Safe, "command {cmd:?}");
    }
}

#[test]
fn assess_real_root_deletion_in_a_chain_still_trips_ps_008() {
    assert_assessment_matches_pattern("echo go && rm / -rf", RiskLevel::Block, "PS-008");
}

// Chains that take the recursive or nested scan path must not leak either.

#[test]
fn assess_flag_of_a_later_command_with_nested_input_does_not_trip_git_push() {
    let s = scanner();
    for cmd in [
        "git push; gh api x -f body=@- <<EOF\nhi\nEOF",
        "git push; gh api x -f body=$(echo hi)",
        "git push; gh api x -f body=`echo hi`",
        "  git push; gh api x -f body=x",
    ] {
        assert_eq!(s.assess(cmd).risk, RiskLevel::Safe, "command {cmd:?}");
    }
}

#[test]
fn assess_real_force_push_in_a_group_or_background_chain_still_warns() {
    for cmd in ["{ git push --force; }", "true & git push --force"] {
        assert_assessment_matches_pattern(cmd, RiskLevel::Warn, "GIT-003");
    }
}
