//! Workspace resolver: the root of the enclosing git worktree, found with
//! `stat` only (never by spawning `git`), plus the gitdirs that may join the
//! `Trusted ceiling` when they provably belong to that worktree.

use std::fs::{self, Metadata};
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// Why no workspace exists for a working directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceUnresolvedReason {
    /// No `.git` found walking up from the working directory.
    NoWorktree,
    /// The worktree root is `/`.
    Root,
    /// The worktree root is the home directory.
    Home,
    /// The worktree root is a strict ancestor of the home directory.
    HomeAncestor,
}

impl WorkspaceUnresolvedReason {
    /// Stable machine-readable name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoWorktree => "no_worktree",
            Self::Root => "root",
            Self::Home => "home",
            Self::HomeAncestor => "home_ancestor",
        }
    }
}

/// A resolved workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    /// Root of the enclosing git worktree (the directory that holds `.git`).
    pub root: PathBuf,
    /// Canonical gitdir and commondir that point back at this worktree.
    /// Empty for a plain `.git` directory (it lies inside `root`).
    pub gitdirs: Vec<PathBuf>,
    /// Set when `.git` is a file whose `gitdir:` target does not point back at
    /// this worktree, or whose `commondir` is not the git layout's parent of
    /// the gitdir; the rejected path is carried for the warning.
    pub unverified_gitdir: Option<PathBuf>,
}

/// Outcome of [`resolve_workspace`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceResolution {
    /// A usable workspace.
    Resolved(Workspace),
    /// No workspace, with the reason.
    Unresolved(WorkspaceUnresolvedReason),
}

/// `st_dev` of a directory, injectable so mount boundaries can be tested.
pub type DeviceFn = Box<dyn Fn(&Path, &Metadata) -> u64>;

/// Environment the resolver reads, injectable so tests are hermetic.
pub struct ResolverInputs {
    /// Home directory used for the degenerate-workspace rule.
    pub home: Option<PathBuf>,
    /// Temporary directories of the computed default (never from `TMPDIR`).
    pub temp_dirs: Vec<PathBuf>,
    /// `st_dev` of a path; the upward search stops where it changes.
    pub device: DeviceFn,
}

/// Resolve the workspace of `cwd` with the real system inputs.
pub fn resolve_workspace(cwd: &Path) -> WorkspaceResolution {
    resolve_workspace_with(cwd, &ResolverInputs::system())
}

/// Resolve the workspace of `cwd` with injected inputs.
///
/// Walks up from `cwd` to the first `.git` (file or directory) using
/// `symlink_metadata` only, stopping where `st_dev` changes. `GIT_DIR`,
/// `GIT_WORK_TREE` and `GIT_COMMON_DIR` are never read.
pub fn resolve_workspace_with(cwd: &Path, inputs: &ResolverInputs) -> WorkspaceResolution {
    use WorkspaceResolution::Unresolved;
    use WorkspaceUnresolvedReason::NoWorktree;

    let Some(root) = find_worktree_root(cwd, inputs) else {
        return Unresolved(NoWorktree);
    };
    if let Some(home) = &inputs.home {
        let reason = classify_root(&root, Some(home)).or_else(|| {
            let canonical = fs::canonicalize(home).ok()?;
            classify_root(&root, Some(&canonical))
        });
        if let Some(reason) = reason {
            return Unresolved(reason);
        }
    } else if let Some(reason) = classify_root(&root, None) {
        return Unresolved(reason);
    }
    WorkspaceResolution::Resolved(workspace_for(root))
}

fn find_worktree_root(cwd: &Path, inputs: &ResolverInputs) -> Option<PathBuf> {
    if !cwd.is_absolute() {
        return None;
    }
    let start_dev = (inputs.device)(cwd, &fs::metadata(cwd).ok()?);
    for dir in cwd.ancestors() {
        let md = fs::metadata(dir).ok()?;
        if (inputs.device)(dir, &md) != start_dev {
            return None;
        }
        if let Ok(git) = fs::symlink_metadata(dir.join(".git"))
            && (git.is_dir() || git.is_file())
        {
            return Some(dir.to_path_buf());
        }
    }
    None
}

/// The degenerate-workspace rule: `/`, the home, and any ancestor of home.
pub(super) fn classify_root(root: &Path, home: Option<&Path>) -> Option<WorkspaceUnresolvedReason> {
    if root.parent().is_none() {
        return Some(WorkspaceUnresolvedReason::Root);
    }
    let home = home?;
    if root == home {
        Some(WorkspaceUnresolvedReason::Home)
    } else if home.starts_with(root) {
        Some(WorkspaceUnresolvedReason::HomeAncestor)
    } else {
        None
    }
}

fn workspace_for(root: PathBuf) -> Workspace {
    let mut ws = Workspace {
        gitdirs: Vec::new(),
        unverified_gitdir: None,
        root,
    };
    let dot_git = ws.root.join(".git");
    if !fs::symlink_metadata(&dot_git).is_ok_and(|m| m.is_file()) {
        return ws;
    }
    let claimed = read_small(&dot_git)
        .and_then(|text| {
            let line = text.lines().next()?;
            Some(ws.root.join(line.strip_prefix("gitdir:")?.trim()))
        })
        // An unparsable `.git` file claims no gitdir: report the file itself.
        .unwrap_or_else(|| dot_git.clone());
    match verified_gitdir(&ws.root, &dot_git, &claimed) {
        Some(gitdir) => {
            let common = read_small(&gitdir.join("commondir"))
                .and_then(|text| fs::canonicalize(gitdir.join(text.trim())).ok());
            // A forged gitdir can name any commondir: accept it only in git's
            // real layout, `<commondir>/worktrees/<name>`.
            let layout_ok = |common: &Path| {
                gitdir.parent().is_some_and(|p| {
                    p.file_name().is_some_and(|n| n == "worktrees") && p.parent() == Some(common)
                })
            };
            let rejected = common.clone().filter(|c| !layout_ok(c));
            ws.gitdirs.push(gitdir);
            if rejected.is_some() {
                ws.unverified_gitdir = rejected;
            } else {
                ws.gitdirs
                    .extend(common.filter(|c| !ws.gitdirs.contains(c)));
            }
        }
        None => ws.unverified_gitdir = Some(claimed),
    }
    ws
}

/// Accept `claimed` only if it points back at this worktree: the `gitdir`
/// file of a linked worktree names `dot_git`, or `core.worktree` of a
/// submodule resolves to `root`.
fn verified_gitdir(root: &Path, dot_git: &Path, claimed: &Path) -> Option<PathBuf> {
    let gitdir = fs::canonicalize(claimed).ok()?;
    let same = |named: &str, expected: &Path| {
        let (Ok(a), Ok(b)) = (
            fs::canonicalize(gitdir.join(named)),
            fs::canonicalize(expected),
        ) else {
            return false;
        };
        a == b
    };
    let linked = read_small(&gitdir.join("gitdir")).is_some_and(|text| same(text.trim(), dot_git));
    let submodule = || {
        read_small(&gitdir.join("config"))
            .and_then(|text| core_worktree(&text))
            .is_some_and(|value| same(&value, root))
    };
    (linked || submodule()).then_some(gitdir)
}

/// `worktree` value of the `[core]` section of a git config file.
fn core_worktree(config: &str) -> Option<String> {
    let mut in_core = false;
    for line in config.lines().map(str::trim) {
        if let Some(section) = line.strip_prefix('[') {
            in_core = section
                .trim_end_matches(']')
                .trim()
                .eq_ignore_ascii_case("core");
        } else if in_core
            && let Some((key, value)) = line.split_once('=')
            && key.trim().eq_ignore_ascii_case("worktree")
        {
            return Some(value.trim().trim_matches('"').to_owned());
        }
    }
    None
}

/// Read a small text file; larger files are truncated, never fully loaded.
fn read_small(path: &Path) -> Option<String> {
    const LIMIT: u64 = 256 * 1024;
    let mut text = String::new();
    fs::File::open(path)
        .ok()?
        .take(LIMIT)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

impl ResolverInputs {
    /// Real passwd home, real `st_dev`, platform temporary directories.
    pub fn system() -> Self {
        Self {
            home: system_home(passwd_home(), std::env::var_os("HOME").map(PathBuf::from)),
            temp_dirs: system_temp_dirs(),
            device: Box::new(|_, md| md.dev()),
        }
    }
}

/// Passwd home wins; the `HOME` variable only counts without a passwd entry.
pub(super) fn system_home(passwd: Option<PathBuf>, env: Option<PathBuf>) -> Option<PathBuf> {
    passwd.or(env)
}

fn passwd_home() -> Option<PathBuf> {
    let mut buf = vec![0u8; 1024];
    loop {
        // SAFETY: `pwd` and `buf` outlive the call; `getpwuid_r` writes only
        // within `buf.len()` bytes and sets `out` to null or to `&mut pwd`.
        let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
        let mut out: *mut libc::passwd = std::ptr::null_mut();
        let rc = unsafe {
            libc::getpwuid_r(
                libc::getuid(),
                &mut pwd,
                buf.as_mut_ptr().cast(),
                buf.len(),
                &mut out,
            )
        };
        if rc == libc::ERANGE && buf.len() < 64 * 1024 {
            buf.resize(buf.len() * 2, 0);
            continue;
        }
        if rc != 0 || out.is_null() || pwd.pw_dir.is_null() {
            return None;
        }
        // SAFETY: `pw_dir` points at a NUL-terminated string inside `buf`.
        let dir = unsafe { std::ffi::CStr::from_ptr(pwd.pw_dir) };
        let dir = PathBuf::from(std::ffi::OsStr::from_bytes(dir.to_bytes()));
        return (!dir.as_os_str().is_empty()).then_some(dir);
    }
}

/// `/tmp`, plus the per-user temporary directory on macOS. `TMPDIR` is never
/// read: an attacker-controlled variable must not widen the ceiling.
fn system_temp_dirs() -> Vec<PathBuf> {
    #[allow(unused_mut)]
    let mut dirs = vec![PathBuf::from("/tmp")];
    #[cfg(target_os = "macos")]
    {
        let mut buf = vec![0u8; libc::PATH_MAX as usize];
        // SAFETY: `buf` is valid for `buf.len()` bytes.
        let n = unsafe {
            libc::confstr(
                libc::_CS_DARWIN_USER_TEMP_DIR,
                buf.as_mut_ptr().cast(),
                buf.len(),
            )
        };
        if n > 1 && n <= buf.len() {
            buf.truncate(n - 1);
            dirs.push(PathBuf::from(std::ffi::OsStr::from_bytes(&buf)));
        }
    }
    dirs
}
