//! Planning tests for the Codex review of PR #529 (#525, ADR-049): the
//! Snapshot plan after `cd` steps, where paths decide between a resolved
//! target, an unresolved one and a duplicate of the process cwd pass.

use super::git_target_plan_tests::{cd_plan, fixture, resolved, unresolved};
use crate::planning::GitTargetUnresolved;
use aegis_scanner::CdUnresolved;

#[test]
fn a_moved_stage_in_the_process_cwd_repository_adds_no_second_target() {
    let fx = fixture();
    std::fs::create_dir_all(fx.cwd.join("sub")).unwrap();
    let (plugins, plan) = cd_plan(&fx, "rm -rf a && cd sub && rm -rf b");

    assert!(plan.targets.is_empty(), "{:?}", plan.targets);
    assert!(plan.process_cwd);
    assert_eq!(plugins, vec!["git"]);
}

#[test]
fn a_moved_git_stage_in_the_process_cwd_repository_adds_no_second_target() {
    let fx = fixture();
    std::fs::create_dir_all(fx.cwd.join("sub")).unwrap();
    let (_, plan) = cd_plan(
        &fx,
        "rm -rf a && cd sub && git filter-branch --index-filter true",
    );

    assert!(plan.targets.is_empty(), "{:?}", plan.targets);
    assert!(plan.process_cwd);
}

#[test]
fn a_moved_stage_in_another_repository_keeps_its_target_beside_the_cwd_pass() {
    let fx = fixture();
    let (_, plan) = cd_plan(&fx, "rm -rf a && cd ../a && rm -rf b");

    assert_eq!(plan.targets, vec![resolved(&fx.a)]);
    assert!(plan.process_cwd);
}

#[test]
fn an_absolute_git_dash_c_resolves_even_after_an_unresolved_cd() {
    let fx = fixture();
    let command = format!(
        "cd \"$X\" && git -C {} filter-branch --index-filter true",
        fx.a.display()
    );
    let (_, plan) = cd_plan(&fx, &command);

    assert_eq!(plan.targets, vec![resolved(&fx.a)]);
    // `cd "$X"` is effect-opaque, so the ADR-016 cwd pass still runs.
    assert!(plan.process_cwd);
}

#[test]
fn an_absolute_git_dash_c_resolves_even_after_a_cd_to_a_missing_directory() {
    let fx = fixture();
    let command = format!(
        "cd ../gone && git -C {} filter-branch --index-filter true",
        fx.a.display()
    );
    let (_, plan) = cd_plan(&fx, &command);

    assert_eq!(plan.targets, vec![resolved(&fx.a)]);
}

#[test]
fn a_relative_git_dash_c_after_an_unresolved_cd_keeps_the_cd_reason() {
    let fx = fixture();
    let (_, plan) = cd_plan(
        &fx,
        "cd \"$X\" && git -C sub filter-branch --index-filter true",
    );

    assert_eq!(
        plan.targets,
        vec![unresolved(GitTargetUnresolved::Cd(
            CdUnresolved::DynamicPath
        ))]
    );
}

#[test]
fn a_plain_git_stage_after_an_unresolved_cd_keeps_the_cd_reason() {
    let fx = fixture();
    let (_, plan) = cd_plan(&fx, "cd \"$X\" && git filter-branch --index-filter true");

    assert_eq!(
        plan.targets,
        vec![unresolved(GitTargetUnresolved::Cd(
            CdUnresolved::DynamicPath
        ))]
    );
}

#[test]
fn an_absolute_cd_after_an_unresolved_one_snapshots_the_known_repository() {
    let fx = fixture();
    let command = format!("cd \"$X\" && cd {} && rm -rf y", fx.a.display());
    let (_, plan) = cd_plan(&fx, &command);

    assert_eq!(plan.targets, vec![resolved(&fx.a)]);
}

#[test]
fn a_directory_changing_launcher_after_a_cd_is_disclosed_for_a_non_git_stage() {
    let fx = fixture();
    let (_, plan) = cd_plan(&fx, "cd ../a && env -C ../b rm -rf x");

    assert!(
        plan.targets
            .contains(&unresolved(GitTargetUnresolved::LauncherChangesDirectory)),
        "{:?}",
        plan.targets
    );
}
