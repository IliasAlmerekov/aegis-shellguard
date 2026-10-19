use super::*;
use aegis_scanner::GitStage;
use tempfile::TempDir;

fn args(dash_c: &[&str], work_tree: Option<&str>, git_dir: Option<&str>) -> GitTargetArgs {
    GitTargetArgs {
        dash_c: dash_c.iter().map(|s| (*s).to_string()).collect(),
        work_tree: work_tree.map(str::to_string),
        git_dir: git_dir.map(str::to_string),
        ..GitTargetArgs::default()
    }
}

#[test]
fn bare_option_is_unresolved_even_with_a_valid_dash_c() {
    let dir = tree();
    let bare = GitTargetArgs {
        bare: true,
        ..args(&["other"], None, None)
    };

    assert_eq!(
        one(bare, &cwd_of(&dir)),
        GitTarget::Unresolved(GitTargetUnresolved::BareRepository)
    );
}

fn stage(target: GitStageTarget) -> GitStage {
    GitStage {
        target,
        dir: StageDir::Process,
        is_git: true,
    }
}

fn one(args: GitTargetArgs, cwd: &CwdState) -> GitTarget {
    resolve_stage(&GitStageTarget::Options(vec![args]), cwd)
}

fn tree() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("a/b/.git")).unwrap();
    std::fs::create_dir_all(dir.path().join("other/.git")).unwrap();
    dir
}

fn cwd_of(dir: &TempDir) -> CwdState {
    CwdState::Resolved(dir.path().canonicalize().unwrap())
}

fn canon(dir: &TempDir, rel: &str) -> PathBuf {
    dir.path().join(rel).canonicalize().unwrap()
}

#[test]
fn relative_dash_c_chain_composes_from_the_cwd() {
    let dir = tree();

    assert_eq!(
        one(args(&["a", "b"], None, None), &cwd_of(&dir)),
        GitTarget::Resolved(canon(&dir, "a/b"))
    );
}

#[test]
fn absolute_dash_c_replaces_the_previous_result() {
    let dir = tree();
    let other = canon(&dir, "other");

    assert_eq!(
        one(
            args(&["a", other.to_str().unwrap()], None, None),
            &cwd_of(&dir)
        ),
        GitTarget::Resolved(other)
    );
}

#[test]
fn work_tree_without_git_dir_is_unresolved() {
    let dir = tree();

    assert_eq!(
        one(args(&[], Some("other"), None), &cwd_of(&dir)),
        GitTarget::Unresolved(GitTargetUnresolved::WorkTreeWithoutGitDir)
    );
}

#[test]
fn work_tree_without_git_dir_after_dash_c_is_unresolved() {
    let dir = tree();

    assert_eq!(
        one(args(&["a"], Some("b"), None), &cwd_of(&dir)),
        GitTarget::Unresolved(GitTargetUnresolved::WorkTreeWithoutGitDir)
    );
}

#[test]
fn work_tree_with_matching_git_dir_applies_after_the_dash_c_chain() {
    let dir = tree();

    assert_eq!(
        one(args(&["a"], Some("b"), Some("b/.git")), &cwd_of(&dir)),
        GitTarget::Resolved(canon(&dir, "a/b"))
    );
}

#[test]
fn empty_dash_c_leaves_the_directory_unchanged() {
    let dir = tree();

    assert_eq!(
        one(args(&["a", ""], None, None), &cwd_of(&dir)),
        GitTarget::Resolved(canon(&dir, "a"))
    );
}

#[test]
fn brace_expansion_and_backslash_are_dynamic() {
    let dir = tree();

    for path in ["{a,b}", "a/{b}", "a\\b"] {
        assert_eq!(
            one(args(&[path], None, None), &cwd_of(&dir)),
            GitTarget::Unresolved(GitTargetUnresolved::DynamicPath),
            "{path}"
        );
    }
}

#[test]
fn git_dir_inside_the_work_tree_is_accepted() {
    let dir = tree();

    assert_eq!(
        one(args(&[], Some("other"), Some("other/.git")), &cwd_of(&dir)),
        GitTarget::Resolved(canon(&dir, "other"))
    );
}

#[test]
fn git_dir_elsewhere_is_unresolved() {
    let dir = tree();

    assert_eq!(
        one(args(&[], Some("other"), Some("a/b/.git")), &cwd_of(&dir)),
        GitTarget::Unresolved(GitTargetUnresolved::GitDirNotInWorkTree)
    );
}

#[test]
fn git_dir_alone_is_unresolved_even_when_it_is_the_cwd_dot_git() {
    let dir = tree();
    std::fs::create_dir(dir.path().join(".git")).unwrap();

    assert_eq!(
        one(args(&[], None, Some(".git")), &cwd_of(&dir)),
        GitTarget::Unresolved(GitTargetUnresolved::GitDirNotInWorkTree)
    );
}

#[test]
fn git_dir_alone_is_unresolved() {
    let dir = tree();

    assert_eq!(
        one(args(&[], None, Some("other/.git")), &cwd_of(&dir)),
        GitTarget::Unresolved(GitTargetUnresolved::GitDirNotInWorkTree)
    );
}

#[test]
fn dynamic_paths_are_unresolved() {
    let dir = tree();

    for path in ["$X", "a/*", "~/repo", "a/[b]", "`pwd`"] {
        assert_eq!(
            one(args(&[path], None, None), &cwd_of(&dir)),
            GitTarget::Unresolved(GitTargetUnresolved::DynamicPath),
            "{path}"
        );
    }
}

#[test]
fn missing_path_is_unresolved() {
    let dir = tree();

    assert_eq!(
        one(args(&["missing"], None, None), &cwd_of(&dir)),
        GitTarget::Unresolved(GitTargetUnresolved::PathNotFound)
    );
}

#[test]
fn relative_dash_c_without_a_cwd_is_unresolved() {
    assert_eq!(
        one(args(&["a"], None, None), &CwdState::Unavailable),
        GitTarget::Unresolved(GitTargetUnresolved::CwdUnavailable)
    );
}

#[test]
fn absolute_dash_c_without_a_cwd_still_resolves() {
    let dir = tree();
    let other = canon(&dir, "other");

    assert_eq!(
        one(
            args(&[other.to_str().unwrap()], None, None),
            &CwdState::Unavailable
        ),
        GitTarget::Resolved(other)
    );
}

#[test]
fn readings_that_disagree_are_unresolved() {
    let dir = tree();
    let stage = GitStageTarget::Options(vec![args(&["a"], None, None), args(&[], None, None)]);

    assert_eq!(
        resolve_stage(&stage, &cwd_of(&dir)),
        GitTarget::Unresolved(GitTargetUnresolved::AmbiguousOptions)
    );
}

#[test]
fn readings_that_agree_resolve_once() {
    let dir = tree();
    let stage = GitStageTarget::Options(vec![args(&["a"], None, None), args(&["a"], None, None)]);

    assert_eq!(
        resolve_stage(&stage, &cwd_of(&dir)),
        GitTarget::Resolved(canon(&dir, "a"))
    );
}

#[test]
fn environment_redirect_is_unresolved() {
    assert_eq!(
        resolve_stage(&GitStageTarget::EnvironmentRedirect, &CwdState::Unavailable),
        GitTarget::Unresolved(GitTargetUnresolved::EnvironmentRedirect)
    );
}

#[test]
fn launcher_that_changes_directory_is_unresolved() {
    assert_eq!(
        resolve_stage(
            &GitStageTarget::LauncherChangesDirectory,
            &CwdState::Unavailable
        ),
        GitTarget::Unresolved(GitTargetUnresolved::LauncherChangesDirectory)
    );
}

#[test]
fn plan_without_redirected_stages_keeps_the_cwd_snapshot() {
    let plan = GitTargetPlan::resolve(&GitStageReport::default(), &CwdState::Unavailable);

    assert_eq!(plan, GitTargetPlan::default());
    assert!(plan.process_cwd);
}

#[test]
fn plan_keeps_each_distinct_target_and_skips_the_cwd_when_unneeded() {
    let dir = tree();
    let report = GitStageReport {
        redirected: vec![
            stage(GitStageTarget::Options(vec![args(&["a"], None, None)])),
            stage(GitStageTarget::Options(vec![args(&["other"], None, None)])),
            stage(GitStageTarget::Options(vec![args(&["other"], None, None)])),
        ],
        needs_process_cwd: false,
    };

    let plan = GitTargetPlan::resolve(&report, &cwd_of(&dir));

    assert_eq!(
        plan.targets,
        vec![
            GitTarget::Resolved(canon(&dir, "a")),
            GitTarget::Resolved(canon(&dir, "other")),
        ]
    );
    assert!(!plan.process_cwd);
    assert!(!plan.has_unresolved());
}

#[test]
fn plan_folds_a_target_equal_to_the_cwd_into_the_cwd_pass() {
    let dir = tree();
    let report = GitStageReport {
        redirected: vec![stage(GitStageTarget::Options(vec![args(
            &["."],
            None,
            None,
        )]))],
        needs_process_cwd: false,
    };

    let plan = GitTargetPlan::resolve(&report, &cwd_of(&dir));

    assert!(plan.targets.is_empty());
    assert!(plan.process_cwd);
}

#[test]
fn plan_reports_an_unresolved_target() {
    let report = GitStageReport {
        redirected: vec![stage(GitStageTarget::Options(vec![args(
            &["$X"],
            None,
            None,
        )]))],
        needs_process_cwd: false,
    };

    let plan = GitTargetPlan::resolve(&report, &CwdState::Unavailable);

    assert!(plan.has_unresolved());
    assert_eq!(plan.resolved_paths().count(), 0);
}

#[test]
fn explain_names_each_target_and_each_unresolved_reason() {
    let plan = GitTargetPlan {
        targets: vec![
            GitTarget::Resolved(PathBuf::from("/work/a")),
            GitTarget::Unresolved(GitTargetUnresolved::PathNotFound),
        ],
        process_cwd: false,
        soft_dirs: Vec::new(),
    };

    assert_eq!(
        plan.explain(),
        vec![
            aegis_explanation::GitTargetExplanation::Resolved {
                path: "/work/a".to_string()
            },
            aegis_explanation::GitTargetExplanation::Unresolved {
                reason: "a path does not exist".to_string()
            },
        ]
    );
}
