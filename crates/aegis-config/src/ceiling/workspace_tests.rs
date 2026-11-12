use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::workspace::{WorkspaceResolution as R, system_home};
use super::*;

const NO_HOME: Option<PathBuf> = None;

/// Inputs whose `st_dev` changes at `boundary`'s parent, so the upward search
/// never leaves the temp dir regardless of what lives above it.
fn inputs(home: Option<PathBuf>, boundary: &Path) -> ResolverInputs {
    let boundary = boundary.to_path_buf();
    ResolverInputs {
        home,
        temp_dirs: vec![PathBuf::from("/tmp")],
        device: Box::new(move |p, _| u64::from(p.starts_with(&boundary))),
    }
}

fn canon(p: &Path) -> PathBuf {
    fs::canonicalize(p).unwrap()
}

fn resolved(r: R) -> Workspace {
    match r {
        R::Resolved(w) => w,
        R::Unresolved(why) => panic!("expected a workspace, got {why:?}"),
    }
}

fn unresolved(r: R) -> WorkspaceUnresolvedReason {
    match r {
        R::Unresolved(why) => why,
        R::Resolved(w) => panic!("expected no workspace, got {w:?}"),
    }
}

fn repo_with_dot_git_dir() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let root = canon(tmp.path()).join("repo");
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::create_dir_all(root.join("src/deep")).unwrap();
    (tmp, root)
}

#[test]
fn workspace_is_the_worktree_root_found_from_a_nested_cwd() {
    let (tmp, root) = repo_with_dot_git_dir();
    let ws = resolved(resolve_workspace_with(
        &root.join("src/deep"),
        &inputs(NO_HOME, tmp.path()),
    ));
    assert_eq!(ws.root, root);
    assert!(ws.gitdirs.is_empty() && ws.unverified_gitdir.is_none());
}

#[test]
fn workspace_is_unresolved_outside_any_worktree() {
    let tmp = TempDir::new().unwrap();
    let cwd = canon(tmp.path());
    let why = unresolved(resolve_workspace_with(&cwd, &inputs(NO_HOME, &cwd)));
    assert_eq!(why, WorkspaceUnresolvedReason::NoWorktree);
}

#[test]
fn workspace_rooted_at_home_is_rejected() {
    let (tmp, root) = repo_with_dot_git_dir();
    let why = unresolved(resolve_workspace_with(
        &root.join("src"),
        &inputs(Some(root.clone()), tmp.path()),
    ));
    assert_eq!(why, WorkspaceUnresolvedReason::Home);
}

#[test]
fn workspace_that_is_an_ancestor_of_home_is_rejected() {
    let (tmp, root) = repo_with_dot_git_dir();
    let why = unresolved(resolve_workspace_with(
        &root.join("src/deep"),
        &inputs(Some(root.join("src")), tmp.path()),
    ));
    assert_eq!(why, WorkspaceUnresolvedReason::HomeAncestor);
}

#[test]
fn workspace_rooted_at_slash_is_rejected() {
    assert_eq!(
        super::workspace::classify_root(Path::new("/"), Some(Path::new("/home/u"))),
        Some(WorkspaceUnresolvedReason::Root)
    );
}

#[test]
fn home_comes_from_passwd_and_only_falls_back_to_the_home_variable() {
    let passwd = Some(PathBuf::from("/home/u"));
    let env = Some(PathBuf::from("/elsewhere"));
    assert_eq!(
        system_home(passwd, env.clone()),
        Some(PathBuf::from("/home/u"))
    );
    assert_eq!(system_home(None, env), Some(PathBuf::from("/elsewhere")));
}

#[test]
fn passwd_home_rejects_a_repo_even_when_the_home_variable_differs() {
    let (tmp, root) = repo_with_dot_git_dir();
    let home = system_home(Some(root.clone()), Some(PathBuf::from("/elsewhere")));
    let why = unresolved(resolve_workspace_with(&root, &inputs(home, tmp.path())));
    assert_eq!(why, WorkspaceUnresolvedReason::Home);
}

#[test]
fn search_stops_where_the_device_changes() {
    let (_tmp, root) = repo_with_dot_git_dir();
    let cwd = root.join("src");
    let mount = cwd.clone();
    let inputs = ResolverInputs {
        home: None,
        temp_dirs: vec![],
        device: Box::new(move |p, _| u64::from(!p.starts_with(&mount))),
    };
    assert_eq!(
        unresolved(resolve_workspace_with(&cwd, &inputs)),
        WorkspaceUnresolvedReason::NoWorktree
    );
}

#[test]
fn linked_worktree_gitdir_and_commondir_join_when_the_back_pointer_matches() {
    let tmp = TempDir::new().unwrap();
    let base = canon(tmp.path());
    let wt = base.join("wt");
    let main_git = base.join("main/.git");
    let admin = main_git.join("worktrees/wt");
    fs::create_dir_all(&admin).unwrap();
    fs::create_dir_all(&wt).unwrap();
    fs::write(wt.join(".git"), format!("gitdir: {}\n", admin.display())).unwrap();
    fs::write(
        admin.join("gitdir"),
        format!("{}\n", wt.join(".git").display()),
    )
    .unwrap();
    fs::write(admin.join("commondir"), "../..\n").unwrap();

    let ws = resolved(resolve_workspace_with(&wt, &inputs(NO_HOME, &base)));
    assert_eq!(ws.root, wt);
    assert_eq!(ws.gitdirs, vec![admin, main_git]);
    assert!(ws.unverified_gitdir.is_none());
}

#[test]
fn submodule_gitdir_joins_when_core_worktree_points_back() {
    let tmp = TempDir::new().unwrap();
    let base = canon(tmp.path());
    let sub = base.join("super/sub");
    let modules = base.join("super/.git/modules/sub");
    fs::create_dir_all(&modules).unwrap();
    fs::create_dir_all(&sub).unwrap();
    fs::write(sub.join(".git"), "gitdir: ../.git/modules/sub\n").unwrap();
    fs::write(
        modules.join("config"),
        "[core]\n\tbare = false\n\tworktree = ../../../sub\n",
    )
    .unwrap();

    let ws = resolved(resolve_workspace_with(&sub, &inputs(NO_HOME, &base)));
    assert_eq!(ws.root, sub);
    assert_eq!(ws.gitdirs, vec![modules]);
}

#[test]
fn forged_gitdir_without_back_pointer_adds_nothing_and_keeps_the_root() {
    let tmp = TempDir::new().unwrap();
    let base = canon(tmp.path());
    let wt = base.join("wt");
    let victim = base.join("victim");
    fs::create_dir_all(&wt).unwrap();
    fs::create_dir_all(&victim).unwrap();
    fs::write(wt.join(".git"), format!("gitdir: {}\n", victim.display())).unwrap();

    let ws = resolved(resolve_workspace_with(&wt, &inputs(NO_HOME, &base)));
    assert_eq!(ws.root, wt);
    assert!(ws.gitdirs.is_empty());
    assert_eq!(ws.unverified_gitdir, Some(victim));
}

#[test]
fn forged_commondir_pointing_outside_the_worktree_layout_is_omitted() {
    let tmp = TempDir::new().unwrap();
    let base = canon(tmp.path());
    let ws_root = base.join("ws");
    let sub = ws_root.join("b");
    let fake = ws_root.join("g");
    let secret = base.join("home/.ssh");
    fs::create_dir_all(&sub).unwrap();
    fs::create_dir_all(&fake).unwrap();
    fs::create_dir_all(&secret).unwrap();
    fs::write(sub.join(".git"), format!("gitdir: {}\n", fake.display())).unwrap();
    fs::write(
        fake.join("gitdir"),
        format!("{}\n", sub.join(".git").display()),
    )
    .unwrap();
    fs::write(fake.join("commondir"), "../../home/.ssh\n").unwrap();

    let ws = resolved(resolve_workspace_with(&sub, &inputs(NO_HOME, &base)));
    assert_eq!(ws.gitdirs, vec![fake]);
    assert_eq!(ws.unverified_gitdir, Some(secret));
}
