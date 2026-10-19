use super::*;
use crate::scanner::{AmbientEnv, CdUnresolved, DangerStageTarget, GitStageReport, StageDir};

// #525: the Snapshot follows the directory a Danger stage runs in once the
// `cd` steps before it are applied. The scanner reports, per Danger stage, the
// literal `cd` steps (or why they cannot be trusted); planning resolves them.
// `rm -rf` (FS-001) is the built-in Danger filesystem rule.

fn report_with(cmd: &str, cdpath_set: bool) -> GitStageReport {
    let scanner = scanner();
    let assessment = scanner.assess(cmd);
    scanner.git_stage_report_with(
        cmd,
        &assessment,
        AmbientEnv {
            git_env: false,
            cdpath_set,
        },
    )
}

fn report(cmd: &str) -> GitStageReport {
    report_with(cmd, false)
}

fn dirs(report: &GitStageReport) -> Vec<StageDir> {
    report.redirected.iter().map(|s| s.dir.clone()).collect()
}

fn chain(steps: &[&str]) -> StageDir {
    StageDir::Chain(steps.iter().map(|s| (*s).to_string()).collect())
}

fn unresolved(reason: CdUnresolved) -> StageDir {
    StageDir::Unresolved(reason)
}

#[test]
fn literal_cd_before_a_danger_stage_is_reported_as_its_directory() {
    let report = report("cd ../other && rm -rf x");

    assert_eq!(dirs(&report), vec![chain(&["../other"])]);
    assert!(!report.needs_process_cwd);
}

#[test]
fn cd_with_double_dash_resolves_like_a_plain_cd() {
    let report = report("cd -- ../other && git filter-branch --index-filter true");

    assert_eq!(dirs(&report), vec![chain(&["../other"])]);
    assert_eq!(report.redirected[0].target, DangerStageTarget::Plain);
    assert!(report.redirected[0].is_git);
    assert!(!report.needs_process_cwd);
}

#[test]
fn cd_then_git_dash_c_keeps_both_the_cd_and_the_option_values() {
    let report = report("cd a && git -C b filter-branch --index-filter true");

    assert_eq!(dirs(&report), vec![chain(&["a"])]);
    assert!(matches!(
        report.redirected[0].target,
        DangerStageTarget::Options(_)
    ));
}

#[test]
fn two_cd_steps_report_two_directories() {
    let report = report("cd a && rm -rf x && cd ../c && rm -rf y");

    assert_eq!(dirs(&report), vec![chain(&["a"]), chain(&["a", "../c"])]);
    assert!(!report.needs_process_cwd);
}

#[test]
fn unresolvable_cd_forms_leave_the_directory_unresolved() {
    let cases = [
        ("cd && rm -rf y", CdUnresolved::UnsupportedForm),
        ("cd - && rm -rf y", CdUnresolved::UnsupportedForm),
        ("cd -P x && rm -rf y", CdUnresolved::UnsupportedForm),
        ("pushd x && rm -rf y", CdUnresolved::UnsupportedForm),
        ("popd && rm -rf y", CdUnresolved::UnsupportedForm),
        ("cd \"$D\" && rm -rf y", CdUnresolved::DynamicPath),
        ("cd ~/x && rm -rf y", CdUnresolved::DynamicPath),
        ("cd x* && rm -rf y", CdUnresolved::DynamicPath),
        ("cd {a,b} && rm -rf y", CdUnresolved::DynamicPath),
        ("cd a\\ b && rm -rf y", CdUnresolved::DynamicPath),
    ];
    for (cmd, reason) in cases {
        let report = report(cmd);

        assert_eq!(dirs(&report), vec![unresolved(reason)], "{cmd}");
        assert!(!report.needs_process_cwd, "{cmd}");
    }
}

#[test]
fn relative_cd_with_cdpath_set_is_unresolved_but_absolute_is_not() {
    let relative = report_with("cd x && rm -rf y", true);
    let absolute = report_with("cd /x && rm -rf y", true);

    assert_eq!(dirs(&relative), vec![unresolved(CdUnresolved::CdPathSet)]);
    assert_eq!(dirs(&absolute), vec![chain(&["/x"])]);
}

#[test]
fn a_cd_not_joined_by_and_leaves_later_stages_unresolved() {
    for cmd in [
        "cd x; rm -rf y",
        "cd x || rm -rf y",
        "cd x\nrm -rf y",
        "cd x & rm -rf y",
        "cd x && true; rm -rf y",
        "true || cd x && rm -rf y",
    ] {
        let report = report(cmd);

        assert_eq!(
            dirs(&report),
            vec![unresolved(CdUnresolved::MayNotRun)],
            "{cmd:?}"
        );
        assert!(!report.needs_process_cwd, "{cmd:?}");
    }
}

#[test]
fn a_cd_pipeline_stage_does_not_change_the_directory() {
    let report = report("cd x | rm -rf y");

    assert!(report.redirected.is_empty());
    assert!(report.needs_process_cwd);
}

#[test]
fn a_subshell_cd_applies_inside_the_group_only() {
    let inside = report("(cd x && rm -rf y)");
    let after = report("(cd x); rm -rf y");

    assert_eq!(dirs(&inside), vec![chain(&["x"])]);
    assert!(!inside.needs_process_cwd);
    assert!(after.redirected.is_empty());
    assert!(after.needs_process_cwd);
}

#[test]
fn cd_hidden_behind_a_wrapper_leaves_later_stages_unresolved() {
    for cmd in [
        "eval 'cd x' && rm -rf y",
        "bash -c 'cd x' && rm -rf y",
        "cd a && source env.sh && rm -rf y",
        "cd a && . env.sh && rm -rf y",
        "builtin cd x && rm -rf y",
    ] {
        let report = report(cmd);

        assert!(
            report
                .redirected
                .iter()
                .any(|s| matches!(s.dir, StageDir::Unresolved(_))),
            "{cmd:?}: {report:?}"
        );
    }
}

#[test]
fn a_cd_inside_a_nested_shell_makes_its_own_danger_stage_unresolved() {
    let report = report("bash -c 'cd x && rm -rf y'");

    assert_eq!(dirs(&report), vec![unresolved(CdUnresolved::Opaque)]);
}

#[test]
fn a_non_git_danger_stage_in_a_resolved_directory_is_not_a_git_stage() {
    let report = report("cd x && rm -rf y");

    assert!(!report.redirected[0].is_git);
    assert_eq!(report.redirected[0].target, DangerStageTarget::Plain);
}

#[test]
fn a_stage_before_the_cd_still_needs_the_process_cwd() {
    let report = report("rm -rf a && cd x && rm -rf b");

    assert_eq!(dirs(&report), vec![chain(&["x"])]);
    assert!(report.needs_process_cwd);
}

#[test]
fn a_command_without_cd_reports_exactly_what_it_did_before() {
    let report = report("rm -rf x; git -C a filter-branch");

    assert!(report.redirected.iter().all(|s| s.dir == StageDir::Process));
    assert!(report.needs_process_cwd);
}

#[test]
fn a_stage_that_is_itself_pushd_reports_unsupported_form_even_with_a_wrapper_inside() {
    for cmd in [
        "pushd x $(eval y) && rm -rf z",
        "pushd $(source f) && rm -rf z",
        "pushd x `eval y` && rm -rf z",
    ] {
        let report = report(cmd);

        assert_eq!(
            dirs(&report),
            vec![unresolved(CdUnresolved::UnsupportedForm)],
            "{cmd:?}"
        );
    }
}

#[test]
fn a_wrapper_after_an_unresolved_cd_keeps_the_first_reason() {
    let report = report("pushd x; eval cd y; rm -rf z");

    assert_eq!(
        dirs(&report),
        vec![unresolved(CdUnresolved::UnsupportedForm)]
    );
}

#[test]
fn quoting_decides_which_cd_path_characters_are_literal() {
    for (cmd, expected) in [
        // Single quotes make every character literal.
        ("cd 'x$y' && rm -rf z", chain(&["x$y"])),
        ("cd 'a*' && rm -rf z", chain(&["a*"])),
        ("cd '~/x' && rm -rf z", chain(&["~/x"])),
        ("cd 'a{b,c}' && rm -rf z", chain(&["a{b,c}"])),
        ("cd 'a\\b' && rm -rf z", chain(&["a\\b"])),
        // Double quotes still expand `$`, a backtick and a backslash, but not
        // globs, braces or a tilde.
        ("cd \"a*\" && rm -rf z", chain(&["a*"])),
        ("cd \"~/x\" && rm -rf z", chain(&["~/x"])),
        ("cd \"a{b,c}\" && rm -rf z", chain(&["a{b,c}"])),
        (
            "cd \"x$y\" && rm -rf z",
            unresolved(CdUnresolved::DynamicPath),
        ),
        (
            "cd \"x`y`\" && rm -rf z",
            unresolved(CdUnresolved::DynamicPath),
        ),
        (
            "cd \"a\\b\" && rm -rf z",
            unresolved(CdUnresolved::DynamicPath),
        ),
        // Unquoted, each of them expands.
        ("cd ~/x && rm -rf z", unresolved(CdUnresolved::DynamicPath)),
        ("cd a* && rm -rf z", unresolved(CdUnresolved::DynamicPath)),
        ("cd x$y && rm -rf z", unresolved(CdUnresolved::DynamicPath)),
        (
            "cd a\\ b && rm -rf z",
            unresolved(CdUnresolved::DynamicPath),
        ),
        // A quoted part does not hide an unquoted one in the same word.
        (
            "cd 'a'$B && rm -rf z",
            unresolved(CdUnresolved::DynamicPath),
        ),
        (
            "cd 'a'\"$B\" && rm -rf z",
            unresolved(CdUnresolved::DynamicPath),
        ),
    ] {
        let report = report(cmd);

        assert_eq!(dirs(&report), vec![expected], "{cmd:?}");
    }
}
