use std::path::{Path, PathBuf};

use aegis_parser::GitTargetArgs;
use aegis_scanner::{CdUnresolved, DangerStage, DangerStageTarget, GitStageReport, StageDir};

use crate::planning::cwd::CwdState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitTargetUnresolved {
    EnvironmentRedirect,
    DynamicPath,
    PathNotFound,
    GitDirNotInWorkTree,
    WorkTreeWithoutGitDir,
    NotAGitRepository,
    CwdUnavailable,
    AmbiguousOptions,
    TooManyOptions,
    UnmappedLauncher,
    LauncherChangesDirectory,
    BareRepository,
    Cd(CdUnresolved),
}

impl GitTargetUnresolved {
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
            Self::BareRepository => "--bare names a repository with no working tree",
            Self::UnmappedLauncher => "git runs behind a launcher whose arguments Aegis cannot map",
            Self::Cd(reason) => match reason {
                CdUnresolved::UnsupportedForm => {
                    "an earlier cd has no plain path (cd, cd -, an option, a directory-stack entry, pushd or popd)"
                }
                CdUnresolved::DynamicPath => {
                    "a path holds a variable, glob, brace or ~ that Aegis cannot expand"
                }
                CdUnresolved::CdPathSet => "CDPATH is set, so a relative cd may land elsewhere",
                CdUnresolved::MayNotRun => "an earlier cd may not have run before this command",
                CdUnresolved::Opaque => {
                    "a cd hides behind eval, source, a launcher or a nested shell"
                }
                CdUnresolved::PhysicalMode => {
                    "Bash physical mode may be on, so .. follows symlinks"
                }
                CdUnresolved::PathChanged => {
                    "an earlier ln or mv may change where a later cd lands"
                }
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitTarget {
    Resolved(PathBuf),
    Unresolved(GitTargetUnresolved),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitTargetPlan {
    pub targets: Vec<GitTarget>,
    pub process_cwd: bool,
    pub opportunistic_dirs: Vec<PathBuf>,
}

impl Default for GitTargetPlan {
    fn default() -> Self {
        Self {
            targets: Vec::new(),
            process_cwd: true,
            opportunistic_dirs: Vec::new(),
        }
    }
}

impl GitTargetPlan {
    #[must_use]
    pub fn has_unresolved(&self) -> bool {
        self.targets
            .iter()
            .any(|target| matches!(target, GitTarget::Unresolved(_)))
    }

    pub fn map_resolved(
        &mut self,
        map: impl Fn(&Path) -> Option<PathBuf>,
        reason: GitTargetUnresolved,
    ) {
        for target in &mut self.targets {
            if let GitTarget::Resolved(path) = target {
                *target = match map(path) {
                    Some(mapped) => GitTarget::Resolved(mapped),
                    None => GitTarget::Unresolved(reason),
                };
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

    pub fn resolved_paths(&self) -> impl Iterator<Item = &Path> {
        self.targets.iter().filter_map(|target| match target {
            GitTarget::Resolved(path) => Some(path.as_path()),
            GitTarget::Unresolved(_) => None,
        })
    }

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

    #[must_use]
    pub fn resolve(report: &GitStageReport, cwd: &CwdState) -> Self {
        if report.redirected.is_empty() {
            return Self::default();
        }

        let mut targets: Vec<GitTarget> = Vec::new();
        let mut opportunistic_dirs: Vec<PathBuf> = Vec::new();
        let mut process_cwd = report.needs_process_cwd;
        let canonical_cwd = cwd.path().and_then(|path| path.canonicalize().ok());

        for stage in &report.redirected {
            let target = match resolve_stage_in_dir(stage, cwd) {
                StageOutcome::Target(target) => target,
                StageOutcome::Opportunistic(path) => {
                    if canonical_cwd.as_deref() == Some(path.as_path()) {
                        process_cwd = true;
                    } else if !opportunistic_dirs.contains(&path) {
                        opportunistic_dirs.push(path);
                    }
                    continue;
                }
            };
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
            opportunistic_dirs,
        }
    }

    pub fn drop_resolved(&mut self, path: &Path) {
        self.targets
            .retain(|target| !matches!(target, GitTarget::Resolved(p) if p == path));
    }

    pub fn push_resolved(&mut self, path: PathBuf) {
        let target = GitTarget::Resolved(path);
        if !self.targets.contains(&target) {
            self.targets.push(target);
        }
    }
}

enum StageOutcome {
    Target(GitTarget),
    Opportunistic(PathBuf),
}

fn resolve_stage_in_dir(stage: &DangerStage, cwd: &CwdState) -> StageOutcome {
    let base = match resolve_dir(&stage.dir, cwd) {
        Ok(base) => base,
        Err(reason) => {
            if stage.is_git
                && let target @ GitTarget::Resolved(_) =
                    resolve_stage(&stage.target, &CwdState::Unavailable)
            {
                return StageOutcome::Target(target);
            }
            return StageOutcome::Target(GitTarget::Unresolved(reason));
        }
    };
    match (&stage.target, stage.is_git, &base) {
        (DangerStageTarget::Plain, false, CwdState::Resolved(path)) => {
            StageOutcome::Opportunistic(path.clone())
        }
        _ => StageOutcome::Target(resolve_stage(&stage.target, &base)),
    }
}

fn resolve_dir(dir: &StageDir, cwd: &CwdState) -> Result<CwdState, GitTargetUnresolved> {
    match dir {
        StageDir::Process => Ok(cwd.clone()),
        StageDir::Unresolved(reason) => Err(GitTargetUnresolved::Cd(*reason)),
        StageDir::Chain(steps) => {
            let mut current: Option<PathBuf> = cwd
                .path()
                .map(|path| start_dir(path, std::env::var_os("PWD").as_deref()));
            for step in steps {
                let joined =
                    join(current.as_deref(), step).ok_or(GitTargetUnresolved::CwdUnavailable)?;
                current = Some(normalize_lexically(&joined));
            }
            let path = current.ok_or(GitTargetUnresolved::CwdUnavailable)?;
            let canonical = path
                .canonicalize()
                .map_err(|_| GitTargetUnresolved::PathNotFound)?;
            Ok(CwdState::Resolved(canonical))
        }
    }
}

fn start_dir(physical: &Path, pwd: Option<&std::ffi::OsStr>) -> PathBuf {
    let Some(pwd) = pwd else {
        return physical.to_path_buf();
    };
    let text = pwd.to_string_lossy();
    let plain = text.starts_with('/') && text.split('/').all(|part| part != "." && part != "..");
    let pwd = Path::new(pwd);
    let same = plain
        && match (pwd.canonicalize(), physical.canonicalize()) {
            (Ok(logical), Ok(physical)) => logical == physical,
            _ => false,
        };
    if same {
        pwd.to_path_buf()
    } else {
        physical.to_path_buf()
    }
}

fn normalize_lexically(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                _ => out.push(".."),
            },
            other => out.push(other),
        }
    }
    out
}

fn resolve_stage(stage: &DangerStageTarget, cwd: &CwdState) -> GitTarget {
    match stage {
        DangerStageTarget::Plain => resolve_args(&GitTargetArgs::default(), cwd),
        DangerStageTarget::EnvironmentRedirect => {
            GitTarget::Unresolved(GitTargetUnresolved::EnvironmentRedirect)
        }
        DangerStageTarget::TooManyOptions => {
            GitTarget::Unresolved(GitTargetUnresolved::TooManyOptions)
        }
        DangerStageTarget::LauncherChangesDirectory => {
            GitTarget::Unresolved(GitTargetUnresolved::LauncherChangesDirectory)
        }
        DangerStageTarget::UnmappedLauncher => {
            GitTarget::Unresolved(GitTargetUnresolved::UnmappedLauncher)
        }
        DangerStageTarget::Options(readings) => {
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
    if args.bare {
        return unresolved(GitTargetUnresolved::BareRepository);
    }
    let all_paths = args
        .dash_c
        .iter()
        .chain(args.work_tree.iter())
        .chain(args.git_dir.iter());
    for path in all_paths {
        if aegis_scanner::path_is_dynamic(path) {
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

    let mut after_c: Option<PathBuf> = cwd.path().map(Path::to_path_buf);
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
mod lexical_tests;
#[cfg(test)]
mod tests;
