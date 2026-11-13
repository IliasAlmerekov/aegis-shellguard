use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::compute::{CeilingOmitReason as Why, CeilingWarning as W, PathOmitted};
use super::*;
use crate::model::SandboxSettings;

struct Fixture {
    _tmp: TempDir,
    base: PathBuf,
    temp: PathBuf,
}

fn fixture() -> Fixture {
    let tmp = TempDir::new().unwrap();
    let base = fs::canonicalize(tmp.path()).unwrap();
    let temp = base.join("tmp");
    fs::create_dir_all(&temp).unwrap();
    Fixture {
        _tmp: tmp,
        base,
        temp,
    }
}

impl Fixture {
    fn inputs(&self) -> ResolverInputs {
        let boundary = self.base.clone();
        ResolverInputs {
            home: Some(PathBuf::from("/nonexistent-home")),
            temp_dirs: vec![self.temp.clone()],
            device: Box::new(move |p, _| u64::from(p.starts_with(&boundary))),
        }
    }

    fn repo(&self) -> PathBuf {
        let repo = self.base.join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir_all(repo.join("src")).unwrap();
        repo
    }

    fn ceiling(&self, sandbox: &SandboxSettings, cwd: &Path) -> TrustedCeiling {
        compute_trusted_ceiling_with(sandbox, cwd, &self.inputs())
    }
}

fn paths(c: &TrustedCeiling) -> Vec<PathBuf> {
    c.roots.iter().map(|r| r.path.clone()).collect()
}

fn listed(paths: &[&str]) -> SandboxSettings {
    SandboxSettings {
        allow_write: Some(
            paths
                .iter()
                .map(|p| WriteRoot::new(PathBuf::from(p), CeilingLayer::Global, "global.toml"))
                .collect(),
        ),
        ..SandboxSettings::default()
    }
}

fn omitted(why: Why, layer: CeilingLayer, location: &str, path: &str) -> W {
    W::PathOmitted(PathOmitted {
        reason: why,
        layer,
        location: location.into(),
        path: PathBuf::from(path),
    })
}

#[test]
fn absent_allow_write_in_a_worktree_is_the_worktree_root_plus_the_temp_dir() {
    let f = fixture();
    let repo = f.repo();
    let c = f.ceiling(&SandboxSettings::default(), &repo.join("src"));
    assert_eq!(paths(&c), vec![repo, f.temp.clone()]);
    assert!(c.warnings.is_empty());
    assert!(c.roots.iter().all(|r| r.layer == CeilingLayer::Computed));
}

#[test]
fn absent_allow_write_outside_a_worktree_is_only_the_temp_dir() {
    let f = fixture();
    let c = f.ceiling(&SandboxSettings::default(), &f.base);
    assert_eq!(paths(&c), vec![f.temp.clone()]);
    assert_eq!(
        c.warnings,
        vec![W::WorkspaceUnresolved {
            reason: WorkspaceUnresolvedReason::NoWorktree,
            cwd: f.base.clone()
        }]
    );
}

#[test]
fn a_worktree_rooted_at_home_leaves_only_the_temp_dir() {
    let f = fixture();
    let repo = f.repo();
    let mut inputs = f.inputs();
    inputs.home = Some(repo.clone());
    let c = compute_trusted_ceiling_with(&SandboxSettings::default(), &repo, &inputs);
    assert_eq!(paths(&c), vec![f.temp.clone()]);
    assert_eq!(
        c.warnings,
        vec![W::WorkspaceUnresolved {
            reason: WorkspaceUnresolvedReason::Home,
            cwd: repo
        }]
    );
}

#[test]
fn linked_worktree_gitdir_and_commondir_join_the_default_ceiling() {
    let f = fixture();
    let wt = f.base.join("wt");
    let main_git = f.base.join("main/.git");
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

    let c = f.ceiling(&SandboxSettings::default(), &wt);
    assert_eq!(paths(&c), vec![wt, admin, main_git, f.temp.clone()]);
    assert!(c.warnings.is_empty());
}

#[test]
fn forged_git_file_keeps_the_root_and_reports_gitdir_unverified() {
    let f = fixture();
    let wt = f.base.join("wt");
    let victim = f.base.join("victim");
    fs::create_dir_all(&wt).unwrap();
    fs::create_dir_all(&victim).unwrap();
    fs::write(wt.join(".git"), format!("gitdir: {}\n", victim.display())).unwrap();

    let c = f.ceiling(&SandboxSettings::default(), &wt);
    assert_eq!(paths(&c), vec![wt, f.temp.clone()]);
    assert_eq!(
        c.warnings,
        vec![omitted(
            Why::GitdirUnverified,
            CeilingLayer::Computed,
            "workspace",
            victim.to_str().unwrap()
        )]
    );
}

#[test]
fn missing_temp_dir_is_omitted_as_a_computed_root() {
    let f = fixture();
    fs::remove_dir(&f.temp).unwrap();
    let c = f.ceiling(&SandboxSettings::default(), &f.base);
    assert!(paths(&c).is_empty());
    assert!(c.warnings.contains(&omitted(
        Why::NotFound,
        CeilingLayer::Computed,
        "temporary directory",
        f.temp.to_str().unwrap()
    )));
}

#[test]
fn a_present_list_overrides_the_default_and_never_adds_to_it() {
    let f = fixture();
    let repo = f.repo();
    let other = f.base.join("other");
    fs::create_dir_all(&other).unwrap();
    let c = f.ceiling(&listed(&[other.to_str().unwrap()]), &repo);
    assert_eq!(paths(&c), vec![other]);
    assert!(c.warnings.is_empty());
}

#[test]
fn an_explicit_empty_list_is_zero_roots_without_fallback() {
    let f = fixture();
    let repo = f.repo();
    let c = f.ceiling(&listed(&[]), &repo);
    assert!(c.roots.is_empty() && c.warnings.is_empty());
}

#[test]
fn a_list_whose_entries_are_all_omitted_stays_empty_with_one_warning_each() {
    let f = fixture();
    let repo = f.repo();
    let missing = f.base.join("missing");
    let c = f.ceiling(
        &listed(&["relative/dir", "/opt/../etc", missing.to_str().unwrap()]),
        &repo,
    );
    assert!(c.roots.is_empty());
    assert_eq!(
        c.warnings,
        vec![
            omitted(
                Why::Relative,
                CeilingLayer::Global,
                "global.toml",
                "relative/dir"
            ),
            omitted(
                Why::ParentDir,
                CeilingLayer::Global,
                "global.toml",
                "/opt/../etc"
            ),
            omitted(
                Why::NotFound,
                CeilingLayer::Global,
                "global.toml",
                missing.to_str().unwrap()
            ),
        ]
    );
}

fn narrowing(entries: &[&Path]) -> SandboxSettings {
    SandboxSettings {
        allow_write: Some(
            entries
                .iter()
                .map(|p| WriteRoot::new(p.to_path_buf(), CeilingLayer::Project, ".aegis.toml"))
                .collect(),
        ),
        allow_write_narrows_computed: true,
        ..SandboxSettings::default()
    }
}

#[test]
fn a_project_list_over_the_computed_default_narrows_it() {
    let f = fixture();
    let repo = f.repo();
    let src = repo.join("src");
    let c = f.ceiling(&narrowing(&[&src]), &repo);
    assert_eq!(paths(&c), vec![src]);
    assert_eq!(c.roots[0].layer, CeilingLayer::Project);
    assert!(c.warnings.is_empty());
}

#[test]
fn a_project_list_cannot_widen_the_computed_default() {
    let f = fixture();
    let repo = f.repo();
    let outside = f.base.join("other");
    fs::create_dir_all(&outside).unwrap();
    let c = f.ceiling(&narrowing(&[&outside]), &repo);
    assert!(c.roots.is_empty());
    assert_eq!(
        c.warnings,
        vec![CeilingWarning::ProjectEntryClamped {
            path: outside,
            location: ".aegis.toml".to_string(),
        }]
    );
}

#[test]
fn git_and_tmp_environment_variables_do_not_change_the_ceiling() {
    let f = fixture();
    let repo = f.repo();
    let before = compute_trusted_ceiling(&SandboxSettings::default(), &repo);
    let keys = ["GIT_WORK_TREE", "GIT_DIR", "GIT_COMMON_DIR", "TMPDIR"];
    let saved: Vec<_> = keys.iter().map(std::env::var_os).collect();
    // SAFETY: this test is the only one in the crate that reads or writes
    // these variables; TMPDIR points at a real directory so concurrent
    // tempfile use in other tests keeps working.
    unsafe {
        std::env::set_var("GIT_WORK_TREE", "/");
        std::env::set_var("GIT_DIR", "/");
        std::env::set_var("GIT_COMMON_DIR", "/");
        std::env::set_var("TMPDIR", "/var/tmp");
    }
    let after = compute_trusted_ceiling(&SandboxSettings::default(), &repo);
    unsafe {
        for (key, value) in keys.iter().zip(saved) {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
    assert_eq!(after, before);
    // The OS temp dirs: `/tmp`, plus the per-user one from `confstr` on macOS.
    let mut expected = vec![repo];
    expected.extend(ResolverInputs::system().temp_dirs);
    assert_eq!(paths(&after), expected);
}

#[test]
fn trusted_roots_are_the_computed_default_when_a_project_list_narrows_it() {
    let f = fixture();
    let repo = f.repo();
    let c = f.ceiling(&narrowing(&[&repo.join("src")]), &repo);
    assert_eq!(
        c.trusted.iter().map(|r| r.path.clone()).collect::<Vec<_>>(),
        vec![repo, f.temp.clone()]
    );
}

#[test]
fn trusted_roots_keep_the_global_list_a_project_list_narrowed() {
    let f = fixture();
    let repo = f.repo();
    let src = repo.join("src");
    let root = |p: &Path, l| WriteRoot::new(p.to_path_buf(), l, "x.toml");
    let sandbox = SandboxSettings {
        allow_write: Some(vec![root(&src, CeilingLayer::Project)]),
        allow_write_trusted: Some(vec![root(&repo, CeilingLayer::Global)]),
        ..SandboxSettings::default()
    };
    let c = f.ceiling(&sandbox, &repo);
    assert_eq!(paths(&c), vec![src]);
    assert_eq!(c.trusted, vec![root(&repo, CeilingLayer::Global)]);
}
