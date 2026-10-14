use super::*;
use crate::scanner::{GitStageReport, GitStageTarget};
use aegis_parser::GitTargetArgs;

// #471: the git Snapshot must cover the working tree git acts on. The scanner
// reports, for each Danger git stage, the raw `-C`/`--work-tree`/`--git-dir`
// values (or why they cannot be trusted) so planning can resolve a Git target.
// `git filter-branch` (GIT-004) is the built-in Danger git rule.

fn report(cmd: &str) -> GitStageReport {
    let scanner = scanner();
    let assessment = scanner.assess(cmd);
    scanner.git_stage_report(cmd, &assessment)
}

fn args(dash_c: &[&str], work_tree: Option<&str>, git_dir: Option<&str>) -> GitTargetArgs {
    GitTargetArgs {
        dash_c: dash_c.iter().map(|s| (*s).to_string()).collect(),
        work_tree: work_tree.map(str::to_string),
        git_dir: git_dir.map(str::to_string),
    }
}

fn only_options(report: &GitStageReport) -> Vec<Vec<GitTargetArgs>> {
    report
        .redirected
        .iter()
        .map(|stage| match &stage.target {
            GitStageTarget::Options(readings) => readings.clone(),
            other => panic!("expected options, got {other:?}"),
        })
        .collect()
}

#[test]
fn dash_c_danger_stage_is_redirected_and_does_not_need_the_process_cwd() {
    let report = report("git -C other filter-branch --index-filter true");

    assert_eq!(
        only_options(&report),
        vec![vec![args(&["other"], None, None)]]
    );
    assert!(!report.needs_process_cwd);
}

#[test]
fn plain_danger_git_stage_needs_the_process_cwd_only() {
    let report = report("git filter-branch --index-filter true");

    assert!(report.redirected.is_empty());
    assert!(report.needs_process_cwd);
}

#[test]
fn global_option_that_does_not_move_the_target_is_a_plain_stage() {
    let report = report("git -c x=y filter-branch");

    assert!(report.redirected.is_empty());
    assert!(report.needs_process_cwd);
}

#[test]
fn non_danger_redirected_stage_is_not_reported() {
    let report = report("git -C other status");

    assert!(report.redirected.is_empty());
    assert!(!report.needs_process_cwd);
}

#[test]
fn two_redirected_stages_are_both_reported() {
    let report = report("git -C a filter-branch && git -C b filter-branch --force");

    assert_eq!(
        only_options(&report),
        vec![
            vec![args(&["a"], None, None)],
            vec![args(&["b"], None, None)]
        ]
    );
    assert!(!report.needs_process_cwd);
}

#[test]
fn a_non_git_danger_stage_adds_the_process_cwd() {
    let report = report("git -C a filter-branch; rm -rf x");

    assert_eq!(only_options(&report), vec![vec![args(&["a"], None, None)]]);
    assert!(report.needs_process_cwd);
}

#[test]
fn a_plain_stage_sharing_the_rule_with_a_redirected_stage_adds_the_process_cwd() {
    let report = report("git -C a filter-branch; git filter-branch");

    assert_eq!(report.redirected.len(), 1);
    assert!(report.needs_process_cwd);
}

#[test]
fn work_tree_and_git_dir_values_are_reported_raw() {
    let report = report("git --work-tree w --git-dir=w/.git filter-branch");

    assert_eq!(
        only_options(&report),
        vec![vec![args(&[], Some("w"), Some("w/.git"))]]
    );
}

#[test]
fn quoted_dynamic_path_is_reported_as_written() {
    let report = report("git -C \"$X\" filter-branch");

    assert_eq!(only_options(&report), vec![vec![args(&["$X"], None, None)]]);
}

#[test]
fn environment_redirect_in_the_prefix_is_flagged_even_without_options() {
    let report = report("GIT_DIR=elsewhere git filter-branch");

    // The normalized segment target drops the assignment and reads as a plain
    // stage, so the process cwd is also reported: an accepted extra Snapshot.
    assert_eq!(report.redirected.len(), 1);
    assert_eq!(
        report.redirected[0].target,
        GitStageTarget::EnvironmentRedirect
    );
}

#[test]
fn launcher_and_nested_shell_stages_are_found() {
    assert_eq!(
        only_options(&report("sudo git -C a filter-branch")),
        vec![vec![args(&["a"], None, None)]]
    );
    assert_eq!(
        only_options(&report("bash -c 'git -C a filter-branch'")),
        vec![vec![args(&["a"], None, None)]]
    );
}

#[test]
fn effect_opaque_command_needs_the_process_cwd() {
    let report = report("git -C a filter-branch; sh ./cleanup.sh");

    assert!(report.needs_process_cwd);
}

#[test]
fn unlisted_option_swallowing_dash_c_reports_both_readings() {
    let report = report("git --newopt -C a filter-branch");

    let stages = only_options(&report);
    assert_eq!(stages.len(), 1);
    assert!(stages[0].contains(&args(&["a"], None, None)));
    assert!(stages[0].contains(&args(&[], None, None)));
}
