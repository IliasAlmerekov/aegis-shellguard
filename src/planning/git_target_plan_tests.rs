//! Planning tests for the Git target (#471): real temp git repositories, the
//! real scanner and the real Snapshot registry applicability checks.

use std::path::{Path, PathBuf};
use std::process::Command;

use aegis_config::AegisConfig;
use aegis_policy::ExecutionTransport;
use aegis_scanner::CdUnresolved;
use aegis_types::{Mode, SnapshotPolicy};
use tempfile::TempDir;

use crate::planning::git_target::{GitTarget, GitTargetUnresolved};
use crate::planning::types::{CwdState, PlanningOutcome, SnapshotPlan};
use crate::planning::{PlanningRequest, plan_with_context};
use crate::runtime::RuntimeContext;

fn git_init(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    let status = Command::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(path)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .status()
        .unwrap();
    assert!(status.success());
}

struct Fixture {
    _root: TempDir,
    cwd: PathBuf,
    a: PathBuf,
    b: PathBuf,
}

fn fixture() -> Fixture {
    let root = TempDir::new().unwrap();
    let base = root.path().canonicalize().unwrap();
    let cwd = base.join("cwd");
    let a = base.join("a");
    let b = base.join("b");
    for dir in [&cwd, &a, &b] {
        git_init(dir);
    }
    Fixture {
        _root: root,
        cwd,
        a,
        b,
    }
}

fn plan_for(fx: &Fixture, command: &str, cwd_state: CwdState) -> SnapshotPlan {
    plan_with_git(fx, command, cwd_state, true)
}

fn plan_with_git(
    fx: &Fixture,
    command: &str,
    cwd_state: CwdState,
    auto_snapshot_git: bool,
) -> SnapshotPlan {
    plan_with_mode(fx, command, cwd_state, auto_snapshot_git, Mode::Protect)
}

fn plan_with_mode(
    fx: &Fixture,
    command: &str,
    cwd_state: CwdState,
    auto_snapshot_git: bool,
    mode: Mode,
) -> SnapshotPlan {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut config = AegisConfig::default();
    config.mode = mode;
    config.snapshot_policy = SnapshotPolicy::Selective;
    config.auto_snapshot_git = auto_snapshot_git;
    config.auto_snapshot_docker = false;
    let context = RuntimeContext::new(config, rt.handle().clone()).unwrap();
    let outcome = std::thread::scope(|scope| {
        scope
            .spawn(|| {
                plan_with_context(
                    &context,
                    PlanningRequest {
                        command,
                        cwd_state,
                        transport: ExecutionTransport::Shell,
                        ci_detected: false,
                    },
                )
            })
            .join()
            .unwrap()
    });
    let _ = &fx.cwd;
    let PlanningOutcome::Planned(plan) = outcome else {
        panic!("expected a normal plan");
    };
    plan.snapshot_plan()
}

fn required(plan: SnapshotPlan) -> (Vec<&'static str>, crate::planning::GitTargetPlan) {
    match plan {
        SnapshotPlan::Required {
            applicable_plugins,
            git_targets,
        } => (applicable_plugins, git_targets),
        SnapshotPlan::NotRequired => panic!("expected a required Snapshot plan"),
    }
}

fn resolved(path: &Path) -> GitTarget {
    GitTarget::Resolved(path.to_path_buf())
}

#[test]
fn dash_c_plans_a_git_snapshot_of_the_other_repository_only() {
    let fx = fixture();
    let command = format!(
        "git -C {} filter-branch --index-filter true",
        fx.a.display()
    );
    let (plugins, targets) = required(plan_for(&fx, &command, CwdState::Resolved(fx.cwd.clone())));

    assert_eq!(targets.targets, vec![resolved(&fx.a)]);
    assert!(!targets.process_cwd);
    assert_eq!(plugins, vec!["git"]);
}

#[test]
fn relative_dash_c_resolves_against_the_cwd() {
    let fx = fixture();
    let nested = fx.cwd.join("nested");
    git_init(&nested);
    let (_, targets) = required(plan_for(
        &fx,
        "git -C nested filter-branch --index-filter true",
        CwdState::Resolved(fx.cwd.clone()),
    ));

    assert_eq!(targets.targets, vec![resolved(&nested)]);
}

#[test]
fn work_tree_and_git_dir_plan_the_work_tree() {
    let fx = fixture();
    let command = format!(
        "git --work-tree {a} --git-dir {a}/.git filter-branch --index-filter true",
        a = fx.a.display()
    );
    let (plugins, targets) = required(plan_for(&fx, &command, CwdState::Resolved(fx.cwd.clone())));

    assert_eq!(targets.targets, vec![resolved(&fx.a)]);
    assert_eq!(plugins, vec!["git"]);
}

#[test]
fn dynamic_path_is_unresolved() {
    // `$X` also degrades the command as effect-opaque (ADR-043, orchestrate),
    // so the cwd may still get its ADR-016 recovery Snapshot; only the Git
    // target is asserted here.
    let fx = fixture();
    let (_, targets) = required(plan_for(
        &fx,
        "git -C \"$X\" filter-branch --index-filter true",
        CwdState::Resolved(fx.cwd.clone()),
    ));

    assert_eq!(
        targets.targets,
        vec![GitTarget::Unresolved(GitTargetUnresolved::DynamicPath)]
    );
}

#[test]
fn missing_path_is_unresolved_and_gets_no_substitute_cwd_snapshot() {
    let fx = fixture();
    let (plugins, targets) = required(plan_for(
        &fx,
        "git -C does-not-exist filter-branch --index-filter true",
        CwdState::Resolved(fx.cwd.clone()),
    ));

    assert_eq!(
        targets.targets,
        vec![GitTarget::Unresolved(GitTargetUnresolved::PathNotFound)]
    );
    assert!(!targets.process_cwd);
    assert!(!plugins.contains(&"git"));
}

#[test]
fn two_redirected_stages_plan_both_targets() {
    let fx = fixture();
    let command = format!(
        "git -C {} filter-branch --index-filter true && git -C {} filter-branch --index-filter true",
        fx.a.display(),
        fx.b.display()
    );
    let (_, targets) = required(plan_for(&fx, &command, CwdState::Resolved(fx.cwd.clone())));

    assert_eq!(targets.targets, vec![resolved(&fx.a), resolved(&fx.b)]);
    assert!(!targets.process_cwd);
}

#[test]
fn a_non_git_danger_stage_adds_the_cwd_snapshot() {
    let fx = fixture();
    let command = format!(
        "git -C {} filter-branch --index-filter true; rm -rf x",
        fx.a.display()
    );
    let (plugins, targets) = required(plan_for(&fx, &command, CwdState::Resolved(fx.cwd.clone())));

    assert_eq!(targets.targets, vec![resolved(&fx.a)]);
    assert!(targets.process_cwd);
    assert_eq!(plugins, vec!["git"]);
}

#[test]
fn unlisted_option_with_two_readings_is_ambiguous() {
    let fx = fixture();
    let (_, targets) = required(plan_for(
        &fx,
        "git --newopt -C a filter-branch --index-filter true",
        CwdState::Resolved(fx.cwd.clone()),
    ));

    assert_eq!(
        targets.targets,
        vec![GitTarget::Unresolved(GitTargetUnresolved::AmbiguousOptions)]
    );
}

#[test]
fn relative_dash_c_without_a_cwd_is_unresolved() {
    let fx = fixture();
    let (_, targets) = required(plan_for(
        &fx,
        "git -C a filter-branch --index-filter true",
        CwdState::Unavailable,
    ));

    assert_eq!(
        targets.targets,
        vec![GitTarget::Unresolved(GitTargetUnresolved::CwdUnavailable)]
    );
}

#[test]
fn environment_redirect_is_unresolved() {
    let fx = fixture();
    let (_, targets) = required(plan_for(
        &fx,
        "GIT_DIR=elsewhere git filter-branch --index-filter true",
        CwdState::Resolved(fx.cwd.clone()),
    ));

    assert!(targets.has_unresolved());
}

#[test]
fn disabled_git_snapshot_still_discloses_an_unresolved_git_target() {
    let fx = fixture();
    let (plugins, targets) = required(plan_with_git(
        &fx,
        "git -C \"$REPO\" filter-branch --index-filter true",
        CwdState::Resolved(fx.cwd.clone()),
        false,
    ));

    assert!(targets.has_unresolved());
    assert!(!plugins.contains(&"git"));
}

#[test]
fn disabled_git_snapshot_runs_nothing_git_for_a_resolved_target() {
    let fx = fixture();
    let command = format!(
        "git -C {} filter-branch --index-filter true",
        fx.a.display()
    );
    let plan = plan_with_git(&fx, &command, CwdState::Resolved(fx.cwd.clone()), false);

    // With no provider left, policy may require no Snapshot at all.
    if let SnapshotPlan::Required {
        applicable_plugins, ..
    } = plan
    {
        assert!(!applicable_plugins.contains(&"git"));
    }
}

#[test]
fn resolved_target_that_is_not_a_git_repository_is_unresolved() {
    let fx = fixture();
    let plain = fx.cwd.parent().unwrap().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let command = format!(
        "git -C {} filter-branch --index-filter true",
        plain.display()
    );
    let (plugins, targets) = required(plan_for(&fx, &command, CwdState::Resolved(fx.cwd.clone())));

    assert_eq!(
        targets.targets,
        vec![GitTarget::Unresolved(
            GitTargetUnresolved::NotAGitRepository
        )]
    );
    assert!(!plugins.contains(&"git"));
}

#[test]
fn audit_mode_takes_no_snapshot_for_an_unresolved_git_target() {
    let fx = fixture();
    let plan = plan_with_mode(
        &fx,
        "git -C \"$REPO\" filter-branch --index-filter true",
        CwdState::Resolved(fx.cwd.clone()),
        true,
        Mode::Audit,
    );

    assert!(matches!(plan, SnapshotPlan::NotRequired));
}

#[test]
fn disabled_git_snapshot_still_reports_a_resolved_target_that_is_not_a_repository() {
    let fx = fixture();
    let plain = fx.cwd.parent().unwrap().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let command = format!(
        "git -C {} filter-branch --index-filter true",
        plain.display()
    );
    let (_, targets) = required(plan_with_git(
        &fx,
        &command,
        CwdState::Resolved(fx.cwd.clone()),
        false,
    ));

    assert_eq!(
        targets.targets,
        vec![GitTarget::Unresolved(
            GitTargetUnresolved::NotAGitRepository
        )]
    );
}

#[test]
fn dash_c_subdirectory_resolves_to_the_repository_top_level() {
    let fx = fixture();
    let sub = fx.a.join("deep/sub");
    std::fs::create_dir_all(&sub).unwrap();
    let command = format!("git -C {} filter-branch --index-filter true", sub.display());
    let (_, targets) = required(plan_for(&fx, &command, CwdState::Resolved(fx.cwd.clone())));

    assert_eq!(targets.targets, vec![resolved(&fx.a)]);
}

#[test]
fn demote_resolved_turns_a_rejected_target_unresolved_and_keeps_the_kept_one() {
    let mut plan = crate::planning::GitTargetPlan {
        targets: vec![
            resolved(Path::new("/keep")),
            resolved(Path::new("/drop")),
            resolved(Path::new("/drop-too")),
        ],
        process_cwd: false,
        opportunistic_dirs: Vec::new(),
    };

    plan.demote_resolved(
        |path| path == Path::new("/keep"),
        GitTargetUnresolved::NotAGitRepository,
    );

    // The two rejected targets collapse into one unresolved entry.
    assert_eq!(
        plan.targets,
        vec![
            resolved(Path::new("/keep")),
            GitTarget::Unresolved(GitTargetUnresolved::NotAGitRepository),
        ]
    );
}

#[test]
fn git_snapshot_applicability_follows_whether_the_directory_is_a_repository() {
    let fx = fixture();
    let plain = fx.cwd.parent().unwrap().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut config = AegisConfig::default();
    config.snapshot_policy = SnapshotPolicy::Selective;
    config.auto_snapshot_git = true;
    config.auto_snapshot_docker = false;
    let context = RuntimeContext::new(config, rt.handle().clone()).unwrap();

    let (in_repo, outside) = rt.block_on(async {
        (
            context.git_snapshot_applicable_at_async(&fx.a).await,
            context.git_snapshot_applicable_at_async(&plain).await,
        )
    });

    assert!(in_repo);
    assert!(!outside);
}

// #525: the Snapshot follows the directory a stage runs in after `cd` steps.

fn cd_plan(fx: &Fixture, command: &str) -> (Vec<&'static str>, crate::planning::GitTargetPlan) {
    required(plan_for(fx, command, CwdState::Resolved(fx.cwd.clone())))
}

fn unresolved(reason: GitTargetUnresolved) -> GitTarget {
    GitTarget::Unresolved(reason)
}

#[test]
fn cd_to_another_repository_plans_its_snapshot_not_the_cwd() {
    let fx = fixture();
    let (plugins, plan) = cd_plan(&fx, "cd ../a && rm -rf x");

    assert_eq!(plan.targets, vec![resolved(&fx.a)]);
    assert!(!plan.process_cwd);
    assert_eq!(plugins, vec!["git"]);
}

#[test]
fn cd_with_double_dash_then_a_git_stage_resolves_to_the_other_repository() {
    let fx = fixture();
    let (_, plan) = cd_plan(&fx, "cd -- ../a && git filter-branch --index-filter true");

    assert_eq!(plan.targets, vec![resolved(&fx.a)]);
    assert!(!plan.process_cwd);
}

#[test]
fn cd_then_dash_c_composes_to_the_nested_repository() {
    let fx = fixture();
    git_init(&fx.a.join("inner"));
    let (_, plan) = cd_plan(
        &fx,
        "cd ../a && git -C inner filter-branch --index-filter true",
    );

    assert_eq!(plan.targets, vec![resolved(&fx.a.join("inner"))]);
}

#[test]
fn two_cd_steps_plan_two_snapshots() {
    let fx = fixture();
    let (_, plan) = cd_plan(&fx, "cd ../a && rm -rf x && cd ../b && rm -rf y");

    assert_eq!(plan.targets, vec![resolved(&fx.a), resolved(&fx.b)]);
    assert!(!plan.process_cwd);
}

#[test]
fn a_subdirectory_of_a_repository_resolves_to_its_top_level() {
    let fx = fixture();
    std::fs::create_dir_all(fx.a.join("deep")).unwrap();
    let (_, plan) = cd_plan(&fx, "cd ../a/deep && rm -rf x");

    assert_eq!(plan.targets, vec![resolved(&fx.a)]);
}

#[test]
fn cd_dotdot_is_followed_lexically_like_the_shell_does() {
    let fx = fixture();
    std::fs::create_dir_all(fx.a.join("deep")).unwrap();
    let link = fx.cwd.parent().unwrap().join("link");
    std::os::unix::fs::symlink(fx.a.join("deep"), &link).unwrap();
    // `link` points into the repository `a`. The shell's `cd ..` from `link`
    // is the logical parent (the fixture root, no repository), so no Snapshot
    // is planned; the physical parent would be the repository `a`.
    let plan = plan_for(
        &fx,
        "cd ../link && cd .. && rm -rf x",
        CwdState::Resolved(fx.cwd.clone()),
    );

    assert_eq!(plan, SnapshotPlan::NotRequired);
}

#[test]
fn unresolvable_cd_forms_are_disclosed_and_never_covered_by_the_cwd() {
    let fx = fixture();
    let cases = [
        (
            "cd && rm -rf y",
            GitTargetUnresolved::Cd(CdUnresolved::UnsupportedForm),
        ),
        (
            "cd - && rm -rf y",
            GitTargetUnresolved::Cd(CdUnresolved::UnsupportedForm),
        ),
        (
            "cd -P ../a && rm -rf y",
            GitTargetUnresolved::Cd(CdUnresolved::UnsupportedForm),
        ),
        (
            "pushd ../a && rm -rf y",
            GitTargetUnresolved::Cd(CdUnresolved::UnsupportedForm),
        ),
        (
            "cd \"$D\" && rm -rf y",
            GitTargetUnresolved::Cd(CdUnresolved::DynamicPath),
        ),
        (
            "cd ~/x && rm -rf y",
            GitTargetUnresolved::Cd(CdUnresolved::DynamicPath),
        ),
        (
            "cd ../a* && rm -rf y",
            GitTargetUnresolved::Cd(CdUnresolved::DynamicPath),
        ),
        (
            "cd ../a; rm -rf y",
            GitTargetUnresolved::Cd(CdUnresolved::MayNotRun),
        ),
        (
            "cd ../a || rm -rf y",
            GitTargetUnresolved::Cd(CdUnresolved::MayNotRun),
        ),
        (
            "cd ../a\nrm -rf y",
            GitTargetUnresolved::Cd(CdUnresolved::MayNotRun),
        ),
    ];
    for (command, reason) in cases {
        let (_, plan) = cd_plan(&fx, command);

        assert_eq!(plan.targets, vec![unresolved(reason)], "{command:?}");
        assert_eq!(plan.resolved_paths().count(), 0, "{command:?}");
        // A dynamic `cd` path (`$D`, `~`, a glob) makes the command
        // effect-opaque (ADR-043), which earns the ADR-016 cwd Snapshot. That
        // is accepted (ADR-049): the unresolved target is still disclosed and
        // halts an unattended run, as `git -C "$X"` does under ADR-048. Every
        // other reason adds no cwd Snapshot.
        // `cd ~/x` and `cd ../a*` expand without a `$`, so the router does not
        // call them effect-opaque and they add no cwd Snapshot.
        let effect_opaque = command.contains('$');
        assert_eq!(plan.process_cwd, effect_opaque, "{command:?}");
    }
}

#[test]
fn pipeline_cd_and_a_closed_subshell_leave_the_process_cwd() {
    let fx = fixture();
    for command in ["cd ../a | rm -rf y", "(cd ../a); rm -rf y"] {
        let (_, plan) = cd_plan(&fx, command);

        assert!(plan.targets.is_empty(), "{command:?}");
        assert!(plan.process_cwd, "{command:?}");
    }
}

#[test]
fn a_subshell_group_snapshots_the_directory_it_changes_to() {
    let fx = fixture();
    let (_, plan) = cd_plan(&fx, "(cd ../a && rm -rf y)");

    // The language router treats a subshell group as effect-opaque, which
    // adds the ADR-016 cwd recovery Snapshot on top; `a` is covered too.
    assert_eq!(plan.targets, vec![resolved(&fx.a)]);
    assert!(plan.process_cwd);
}

#[test]
fn quoted_cd_paths_name_the_directory_that_is_literally_spelled() {
    let fx = fixture();
    let base = fx.cwd.parent().unwrap();
    let dollar = base.join("x$y");
    let star = base.join("a*");
    let tilde = fx.cwd.join("~");
    for dir in [&dollar, &star, &tilde] {
        git_init(dir);
    }
    for (command, expected) in [
        ("cd '../x$y' && rm -rf z", &dollar),
        ("cd '../a*' && rm -rf z", &star),
        ("cd \"~\" && rm -rf z", &tilde),
    ] {
        let (_, plan) = cd_plan(&fx, command);

        assert_eq!(plan.targets, vec![resolved(expected)], "{command:?}");
    }
}

#[test]
fn an_expanding_cd_path_stays_unresolved_even_when_a_literal_directory_exists() {
    let fx = fixture();
    git_init(&fx.cwd.parent().unwrap().join("x$y"));
    for command in ["cd \"../x$y\" && rm -rf z", "cd ~/x && rm -rf z"] {
        let (_, plan) = cd_plan(&fx, command);

        assert_eq!(
            plan.targets,
            vec![unresolved(GitTargetUnresolved::Cd(
                CdUnresolved::DynamicPath
            ))],
            "{command:?}"
        );
    }
}

#[test]
fn a_git_dash_c_path_with_a_quoted_dollar_stays_unresolved() {
    // `-C` values reach planning after the parser removed their quotes, so a
    // literal `$` cannot be told from an expansion: stay conservative.
    let fx = fixture();
    git_init(&fx.cwd.parent().unwrap().join("x$y"));
    let (_, plan) = cd_plan(&fx, "git -C '../x$y' filter-branch --index-filter true");

    assert_eq!(
        plan.targets,
        vec![unresolved(GitTargetUnresolved::DynamicPath)]
    );
}

#[test]
fn a_non_git_stage_in_a_plain_directory_takes_no_snapshot_and_no_disclosure() {
    let fx = fixture();
    let plain = fx.cwd.parent().unwrap().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let plan = plan_for(
        &fx,
        "cd ../plain && rm -rf x",
        CwdState::Resolved(fx.cwd.clone()),
    );

    // No git Snapshot applies, so the plan holds nothing to disclose or halt on.
    assert_eq!(plan, SnapshotPlan::NotRequired);
}

#[test]
fn a_git_stage_in_a_plain_directory_is_an_unresolved_target() {
    let fx = fixture();
    let plain = fx.cwd.parent().unwrap().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let (_, plan) = cd_plan(&fx, "cd ../plain && git filter-branch --index-filter true");

    assert_eq!(
        plan.targets,
        vec![unresolved(GitTargetUnresolved::NotAGitRepository)]
    );
}

#[test]
fn a_directory_that_does_not_exist_yet_is_unresolved() {
    let fx = fixture();
    for command in ["mkdir n && cd n && rm -rf x", "cd ../gone && rm -rf x"] {
        let (_, plan) = cd_plan(&fx, command);

        assert_eq!(
            plan.targets,
            vec![unresolved(GitTargetUnresolved::PathNotFound)],
            "{command:?}"
        );
    }
}

#[test]
fn audit_mode_takes_no_snapshot_for_an_unresolved_cd_and_explains_it() {
    let fx = fixture();
    let plan = plan_with_mode(
        &fx,
        "cd \"$D\" && rm -rf x",
        CwdState::Resolved(fx.cwd.clone()),
        true,
        Mode::Audit,
    );

    assert_eq!(plan, SnapshotPlan::NotRequired);
}

#[test]
fn a_command_without_cd_keeps_the_pre_525_plan() {
    let fx = fixture();
    let (_, plan) = cd_plan(&fx, "rm -rf x");

    assert_eq!(plan, crate::planning::GitTargetPlan::default());
}

#[test]
fn a_stage_in_the_cwd_after_cd_dot_folds_into_the_cwd_snapshot() {
    let fx = fixture();
    let (_, plan) = cd_plan(&fx, "cd . && rm -rf x");

    assert!(plan.targets.is_empty());
    assert!(plan.process_cwd);
}

#[test]
fn docker_and_database_providers_keep_using_the_process_cwd_after_cd() {
    let fx = fixture();
    let plugins = |command: &str| {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut config = AegisConfig::default();
        config.snapshot_policy = SnapshotPolicy::Full;
        config.auto_snapshot_git = true;
        config.auto_snapshot_docker = true;
        let context = RuntimeContext::new(config, rt.handle().clone()).unwrap();
        let outcome = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    plan_with_context(
                        &context,
                        PlanningRequest {
                            command,
                            cwd_state: CwdState::Resolved(fx.cwd.clone()),
                            transport: ExecutionTransport::Shell,
                            ci_detected: false,
                        },
                    )
                })
                .join()
                .unwrap()
        });
        let PlanningOutcome::Planned(plan) = outcome else {
            panic!("expected a normal plan");
        };
        let (plugins, _) = required(plan.snapshot_plan());
        plugins
            .into_iter()
            .filter(|name| *name != "git")
            .collect::<Vec<_>>()
    };

    assert_eq!(plugins("cd ../a && rm -rf x"), plugins("rm -rf x"));
}
