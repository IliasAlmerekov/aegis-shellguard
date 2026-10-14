//! Git target resolution for the git Snapshot (#471, ADR-048).
//!
//! The git `Snapshot plugin` must capture the working tree a `git` command
//! acts on, not the process cwd. The scanner reports, per `Danger` git stage,
//! the raw `-C`, `--work-tree` and `--git-dir` values; this module turns them
//! into a [`GitTarget`]: a canonical directory, or an explicit "unresolved"
//! marker with the reason. An unresolved target is never replaced by a cwd
//! Snapshot, and no resolution happens on the safe path.

use std::path::{Path, PathBuf};

use aegis_parser::GitTargetArgs;
use aegis_scanner::{GitStageReport, GitStageTarget};

use crate::planning::types::CwdState;

/// Why a Git target could not be resolved with confidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitTargetUnresolved {
    /// `GIT_DIR=` or `GIT_WORK_TREE=` redirects git from the environment.
    EnvironmentRedirect,
    /// A path holds `$`, a glob or `~`, so its value is not known statically.
    DynamicPath,
    /// A path does not exist, so git has no tree there to act on.
    PathNotFound,
    /// `--git-dir` is given without `--work-tree`, or does not canonicalize
    /// to `<work tree>/.git`.
    GitDirNotInWorkTree,
    /// `--work-tree` is given without `--git-dir`: git still finds the
    /// repository from the `-C` directory, so the work tree is not the target.
    WorkTreeWithoutGitDir,
    /// The resolved directory is not a git repository the git Snapshot can
    /// cover.
    NotAGitRepository,
    /// A relative path needs the process cwd, which is unavailable.
    CwdUnavailable,
    /// Two readings of an unlisted global option give different targets.
    AmbiguousOptions,
    /// More unrecognized global options than the scanner walks.
    TooManyOptions,
    /// The git stage sits behind a launcher whose argv Aegis cannot map back.
    UnmappedLauncher,
    /// A launcher (`env -C`, `sudo -D`) changes the directory before git runs.
    LauncherChangesDirectory,
}

impl GitTargetUnresolved {
    /// Short operator-facing reason, used by the dialog and the audit entry.
    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Self::EnvironmentRedirect => {
                "GIT_DIR or GIT_WORK_TREE redirects git from the environment"
            }
            Self::DynamicPath => {
                "a path holds a variable, glob, brace or ~ that Aegis cannot expand"
            }
            Self::NotAGitRepository => "the target directory is not a git repository",
            Self::PathNotFound => "a path does not exist",
            Self::GitDirNotInWorkTree => "--git-dir is not <work tree>/.git",
            Self::WorkTreeWithoutGitDir => "--work-tree is given without a matching --git-dir",
            Self::CwdUnavailable => {
                "a relative path needs the working directory, which is unavailable"
            }
            Self::AmbiguousOptions => {
                "two readings of an unknown git option give different targets"
            }
            Self::TooManyOptions => "too many unrecognized git global options",
            Self::LauncherChangesDirectory => {
                "a launcher changes the working directory before git runs"
            }
            Self::UnmappedLauncher => "git runs behind a launcher whose arguments Aegis cannot map",
        }
    }
}

/// The working tree one `git` stage acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitTarget {
    /// A canonical directory the git Snapshot covers.
    Resolved(PathBuf),
    /// No Snapshot covers this stage; the reason is disclosed.
    Unresolved(GitTargetUnresolved),
}

/// The git Snapshots a plan asks for.
///
/// `process_cwd` says whether the git plugin also runs at the process cwd:
/// true whenever some part of the command needs a Snapshot without a git
/// redirect. The default, used when no stage redirects git, keeps the
/// pre-#471 behaviour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitTargetPlan {
    /// One entry per distinct redirected git stage.
    pub targets: Vec<GitTarget>,
    /// Whether the git plugin runs at the process cwd.
    pub process_cwd: bool,
}

impl Default for GitTargetPlan {
    fn default() -> Self {
        Self {
            targets: Vec::new(),
            process_cwd: true,
        }
    }
}

impl GitTargetPlan {
    /// Whether some Git target could not be resolved.
    #[must_use]
    pub fn has_unresolved(&self) -> bool {
        self.targets
            .iter()
            .any(|target| matches!(target, GitTarget::Unresolved(_)))
    }

    /// Replace each resolved target that `keep` rejects with an unresolved
    /// one carrying `reason`. Used for a target the git plugin cannot cover.
    pub fn demote_resolved(&mut self, keep: impl Fn(&Path) -> bool, reason: GitTargetUnresolved) {
        for target in &mut self.targets {
            if let GitTarget::Resolved(path) = target
                && !keep(path)
            {
                *target = GitTarget::Unresolved(reason);
            }
        }
        let mut seen: Vec<GitTarget> = Vec::new();
        self.targets.retain(|target| {
            let fresh = !seen.contains(target);
            if fresh {
                seen.push(target.clone());
            }
            fresh
        });
    }

    /// The canonical directories the git plugin must snapshot.
    pub fn resolved_paths(&self) -> impl Iterator<Item = &Path> {
        self.targets.iter().filter_map(|target| match target {
            GitTarget::Resolved(path) => Some(path.as_path()),
            GitTarget::Unresolved(_) => None,
        })
    }

    /// The targets as the dialog and the audit entry disclose them.
    #[must_use]
    pub fn explain(&self) -> Vec<aegis_explanation::GitTargetExplanation> {
        self.targets
            .iter()
            .map(|target| match target {
                GitTarget::Resolved(path) => aegis_explanation::GitTargetExplanation::Resolved {
                    path: path.display().to_string(),
                },
                GitTarget::Unresolved(reason) => {
                    aegis_explanation::GitTargetExplanation::Unresolved {
                        reason: reason.description().to_string(),
                    }
                }
            })
            .collect()
    }

    /// Resolve the targets of every redirected git stage in `report`.
    ///
    /// Touches the filesystem (canonicalization), so callers run it only for
    /// a command that already needs a recovery backstop.
    #[must_use]
    pub fn resolve(report: &GitStageReport, cwd: &CwdState) -> Self {
        if report.redirected.is_empty() {
            return Self::default();
        }

        let mut targets: Vec<GitTarget> = Vec::new();
        let mut process_cwd = report.needs_process_cwd;
        let canonical_cwd = match cwd {
            CwdState::Resolved(path) => path.canonicalize().ok(),
            CwdState::Unavailable => None,
        };

        for stage in &report.redirected {
            let target = resolve_stage(&stage.target, cwd);
            // A target that is the process cwd is covered by the cwd pass.
            if let GitTarget::Resolved(path) = &target
                && canonical_cwd.as_deref() == Some(path.as_path())
            {
                process_cwd = true;
                continue;
            }
            if !targets.contains(&target) {
                targets.push(target);
            }
        }

        Self {
            targets,
            process_cwd,
        }
    }
}

fn resolve_stage(stage: &GitStageTarget, cwd: &CwdState) -> GitTarget {
    match stage {
        GitStageTarget::EnvironmentRedirect => {
            GitTarget::Unresolved(GitTargetUnresolved::EnvironmentRedirect)
        }
        GitStageTarget::TooManyOptions => {
            GitTarget::Unresolved(GitTargetUnresolved::TooManyOptions)
        }
        GitStageTarget::LauncherChangesDirectory => {
            GitTarget::Unresolved(GitTargetUnresolved::LauncherChangesDirectory)
        }
        GitStageTarget::UnmappedLauncher => {
            GitTarget::Unresolved(GitTargetUnresolved::UnmappedLauncher)
        }
        GitStageTarget::Options(readings) => {
            let mut outcomes: Vec<GitTarget> = Vec::new();
            for args in readings {
                let outcome = resolve_args(args, cwd);
                if !outcomes.contains(&outcome) {
                    outcomes.push(outcome);
                }
            }
            match outcomes.len() {
                1 => outcomes.remove(0),
                _ => GitTarget::Unresolved(GitTargetUnresolved::AmbiguousOptions),
            }
        }
    }
}

fn is_dynamic(path: &str) -> bool {
    path.contains(['$', '*', '?', '[', ']', '~', '`', '{', '}', '\\'])
}

/// Join `path` onto `base`, `None` when a relative path has no base.
fn join(base: Option<&Path>, path: &str) -> Option<PathBuf> {
    let path = Path::new(path);
    if path.is_absolute() {
        Some(path.to_path_buf())
    } else {
        base.map(|base| base.join(path))
    }
}

fn resolve_args(args: &GitTargetArgs, cwd: &CwdState) -> GitTarget {
    let unresolved = GitTarget::Unresolved;
    let all_paths = args
        .dash_c
        .iter()
        .chain(args.work_tree.iter())
        .chain(args.git_dir.iter());
    for path in all_paths {
        if is_dynamic(path) {
            return unresolved(GitTargetUnresolved::DynamicPath);
        }
    }
    let non_chain_empty = args
        .work_tree
        .iter()
        .chain(args.git_dir.iter())
        .any(String::is_empty);
    if non_chain_empty {
        return unresolved(GitTargetUnresolved::PathNotFound);
    }
    match (&args.work_tree, &args.git_dir) {
        (None, Some(_)) => return unresolved(GitTargetUnresolved::GitDirNotInWorkTree),
        (Some(_), None) => return unresolved(GitTargetUnresolved::WorkTreeWithoutGitDir),
        _ => {}
    }

    let mut after_c: Option<PathBuf> = match cwd {
        CwdState::Resolved(path) => Some(path.clone()),
        CwdState::Unavailable => None,
    };
    // `git -C ""` is a no-op, so an empty value leaves the directory as is.
    for path in args.dash_c.iter().filter(|path| !path.is_empty()) {
        after_c = join(after_c.as_deref(), path);
        if after_c.is_none() {
            return unresolved(GitTargetUnresolved::CwdUnavailable);
        }
    }
    let work_tree = match &args.work_tree {
        Some(path) => join(after_c.as_deref(), path),
        None => after_c.clone(),
    };
    let Some(work_tree) = work_tree else {
        return unresolved(GitTargetUnresolved::CwdUnavailable);
    };
    let Ok(work_tree) = work_tree.canonicalize() else {
        return unresolved(GitTargetUnresolved::PathNotFound);
    };

    if let Some(git_dir) = &args.git_dir {
        let Some(git_dir) = join(after_c.as_deref(), git_dir) else {
            return unresolved(GitTargetUnresolved::CwdUnavailable);
        };
        let Ok(git_dir) = git_dir.canonicalize() else {
            return unresolved(GitTargetUnresolved::PathNotFound);
        };
        match work_tree.join(".git").canonicalize() {
            Ok(expected) if expected == git_dir => {}
            _ => return unresolved(GitTargetUnresolved::GitDirNotInWorkTree),
        }
    }

    GitTarget::Resolved(work_tree)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_scanner::GitStage;
    use tempfile::TempDir;

    fn args(dash_c: &[&str], work_tree: Option<&str>, git_dir: Option<&str>) -> GitTargetArgs {
        GitTargetArgs {
            dash_c: dash_c.iter().map(|s| (*s).to_string()).collect(),
            work_tree: work_tree.map(str::to_string),
            git_dir: git_dir.map(str::to_string),
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
        let stage =
            GitStageTarget::Options(vec![args(&["a"], None, None), args(&["a"], None, None)]);

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
                GitStage {
                    target: GitStageTarget::Options(vec![args(&["a"], None, None)]),
                },
                GitStage {
                    target: GitStageTarget::Options(vec![args(&["other"], None, None)]),
                },
                GitStage {
                    target: GitStageTarget::Options(vec![args(&["other"], None, None)]),
                },
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
            redirected: vec![GitStage {
                target: GitStageTarget::Options(vec![args(&["."], None, None)]),
            }],
            needs_process_cwd: false,
        };

        let plan = GitTargetPlan::resolve(&report, &cwd_of(&dir));

        assert!(plan.targets.is_empty());
        assert!(plan.process_cwd);
    }

    #[test]
    fn plan_reports_an_unresolved_target() {
        let report = GitStageReport {
            redirected: vec![GitStage {
                target: GitStageTarget::Options(vec![args(&["$X"], None, None)]),
            }],
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
}
