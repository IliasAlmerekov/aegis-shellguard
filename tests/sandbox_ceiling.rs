//! End-to-end tests for the computed Trusted ceiling (#242, ADR-030): an
//! absent `[sandbox] allow_write` confines a command to the git worktree and
//! the temporary directory.
//!
//! Tests that run a confined command skip, never fail, when this host has no
//! usable confinement backend. Worktrees live under `/var/tmp` because every
//! `TempDir` is under `/tmp`, which the default ceiling always admits.
#![cfg(target_os = "linux")]

mod support;

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

use tempfile::TempDir;

use support::{base_command, confinement_available, write_global_config};

const ENABLED: &str = "[sandbox]\nenabled = true\n";

/// A directory outside `/tmp`, or `None` when `/var/tmp` is unusable.
fn outside_tmp() -> Option<TempDir> {
    TempDir::new_in("/var/tmp").ok()
}

/// `git` with every inherited repository variable and config removed.
fn git(home: &Path, dir: &Path, args: &[&str]) {
    let output = support::git_command(dir)
        .env("HOME", home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .args(["-c", "protocol.file.allow=always"])
        .args(args)
        .output()
        .expect("git must be installed");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_repo(home: &Path, dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    git(home, dir, &["init", "-q", "-b", "main"]);
    git(home, dir, &["commit", "-q", "--allow-empty", "-m", "init"]);
}

fn run_in(home: &Path, cwd: &Path, command: &str) -> Output {
    base_command(home)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .current_dir(cwd)
        .args(["-c", command])
        .output()
        .unwrap()
}

fn hermetic_home() -> TempDir {
    let home = TempDir::new().unwrap();
    write_global_config(home.path(), ENABLED);
    home
}

fn status(home: &Path, cwd: &Path) -> String {
    let output = base_command(home)
        .current_dir(cwd)
        .arg("status")
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn unique_tmp_file(tag: &str) -> PathBuf {
    PathBuf::from(format!("/tmp/aegis-ceiling-{}-{tag}", std::process::id()))
}

#[test]
fn default_ceiling_admits_the_worktree_and_tmp_and_nothing_else() {
    let Some(base) = outside_tmp() else { return };
    if !confinement_available() {
        return;
    }
    let home = hermetic_home();
    let repo = base.path().join("repo");
    init_repo(home.path(), &repo);
    let elsewhere = base.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    let tmp_file = unique_tmp_file("default");

    let command = format!(
        "echo a > {repo}/inside.txt; echo b > {tmp}; echo c > {elsewhere}/outside.txt",
        repo = repo.display(),
        tmp = tmp_file.display(),
        elsewhere = elsewhere.display(),
    );
    let output = run_in(home.path(), &repo, &command);

    let tmp_written = tmp_file.exists();
    let _ = fs::remove_file(&tmp_file);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(repo.join("inside.txt").exists(), "worktree write: {stderr}");
    assert!(tmp_written, "temporary directory write: {stderr}");
    assert!(!elsewhere.join("outside.txt").exists(), "outside: {stderr}");
}

#[test]
fn outside_a_worktree_only_tmp_is_writable_and_status_says_why() {
    let Some(base) = outside_tmp() else { return };
    let home = hermetic_home();
    let cwd = base.path().join("plain");
    fs::create_dir_all(&cwd).unwrap();

    let shown = status(home.path(), &cwd);
    assert!(shown.contains("workspace_unresolved"), "{shown}");
    assert!(shown.contains("no_worktree"), "{shown}");

    if !confinement_available() {
        return;
    }
    let tmp_file = unique_tmp_file("plain");
    let command = format!(
        "echo a > {}; echo b > {}/x.txt",
        tmp_file.display(),
        cwd.display()
    );
    let output = run_in(home.path(), &cwd, &command);
    let tmp_written = tmp_file.exists();
    let _ = fs::remove_file(&tmp_file);
    assert!(tmp_written, "{}", String::from_utf8_lossy(&output.stderr));
    assert!(!cwd.join("x.txt").exists());
}

#[test]
fn git_commit_succeeds_from_a_linked_worktree_under_the_default_ceiling() {
    let Some(base) = outside_tmp() else { return };
    if !confinement_available() {
        return;
    }
    let home = hermetic_home();
    let main = base.path().join("main");
    let linked = base.path().join("linked");
    init_repo(home.path(), &main);
    git(
        home.path(),
        &main,
        &["worktree", "add", "-q", linked.to_str().unwrap()],
    );

    let output = run_in(
        home.path(),
        &linked,
        "git commit -q --allow-empty -m from-linked",
    );

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn git_commit_succeeds_from_a_submodule_under_the_default_ceiling() {
    let Some(base) = outside_tmp() else { return };
    if !confinement_available() {
        return;
    }
    let home = hermetic_home();
    let lib = base.path().join("lib");
    let sup = base.path().join("super");
    init_repo(home.path(), &lib);
    init_repo(home.path(), &sup);
    git(
        home.path(),
        &sup,
        &["submodule", "add", "-q", lib.to_str().unwrap(), "sub"],
    );
    let sub = sup.join("sub");

    let output = run_in(home.path(), &sub, "git commit -q --allow-empty -m from-sub");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn git_work_tree_and_tmpdir_variables_do_not_widen_the_ceiling() {
    let Some(base) = outside_tmp() else { return };
    if !confinement_available() {
        return;
    }
    let home = hermetic_home();
    let repo = base.path().join("repo");
    init_repo(home.path(), &repo);
    let elsewhere = base.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();

    let command = format!(
        "echo a > {repo}/inside.txt; echo c > {elsewhere}/outside.txt",
        repo = repo.display(),
        elsewhere = elsewhere.display()
    );
    let output = base_command(home.path())
        .env("GIT_WORK_TREE", "/")
        .env("TMPDIR", "/")
        .current_dir(&repo)
        .args(["-c", &command])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(repo.join("inside.txt").exists(), "{stderr}");
    assert!(!elsewhere.join("outside.txt").exists(), "{stderr}");
}

#[test]
fn status_shows_no_workspace_warning_when_in_audit_mode() {
    let Some(base) = outside_tmp() else { return };
    let home = TempDir::new().unwrap();
    write_global_config(home.path(), "mode = \"Audit\"\n");
    let cwd = base.path().join("plain");
    fs::create_dir_all(&cwd).unwrap();

    let shown = status(home.path(), &cwd);

    assert!(!shown.contains("workspace_unresolved"), "{shown}");
}

#[test]
fn audit_mode_builds_no_ceiling_so_no_git_search_runs() {
    let mut config = aegis_config::AegisConfig::default();
    config.mode = aegis_types::Mode::Audit;
    assert!(aegis::runtime::sandbox_ceiling(&config, Path::new("/tmp")).is_none());
    assert!(
        aegis::runtime::RuntimeConfig::from(&config)
            .sandbox
            .is_none()
    );
}

#[test]
fn status_reports_a_project_root_that_resolves_outside_the_trusted_roots() {
    let Some(base) = outside_tmp() else { return };
    let home = TempDir::new().unwrap();
    let ws = base.path().join("ws");
    let outside = base.path().join("outside");
    fs::create_dir_all(&ws).unwrap();
    fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, ws.join("link")).unwrap();
    write_global_config(
        home.path(),
        &format!(
            "[sandbox]\nenabled = true\nallow_write = [\"{}\"]\n",
            ws.display()
        ),
    );
    let project = ws.join(".aegis.toml");
    fs::write(
        &project,
        format!("[sandbox]\nallow_write = [\"{}/link\"]\n", ws.display()),
    )
    .unwrap();

    let shown = status(home.path(), &ws);

    let line = shown
        .lines()
        .find(|l| l.contains("trusted_ceiling_path_omitted"))
        .unwrap_or_else(|| panic!("no omission line in: {shown}"));
    assert!(line.contains(project.to_str().unwrap()), "{line}");
    assert!(line.contains("project layer"), "{line}");
    assert!(line.contains("outside_trusted_ceiling"), "{line}");
}

#[test]
fn config_validate_reports_the_enforcement_omission_without_failing() {
    let Some(base) = outside_tmp() else { return };
    let home = TempDir::new().unwrap();
    let ws = base.path().join("ws");
    let outside = base.path().join("outside");
    fs::create_dir_all(&ws).unwrap();
    fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, ws.join("link")).unwrap();
    write_global_config(
        home.path(),
        &format!(
            "[sandbox]\nenabled = true\nallow_write = [\"{}\"]\n",
            ws.display()
        ),
    );
    fs::write(
        ws.join(".aegis.toml"),
        format!("[sandbox]\nallow_write = [\"{}/link\"]\n", ws.display()),
    )
    .unwrap();

    let output = base_command(home.path())
        .current_dir(&ws)
        .args(["config", "validate"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("outside_trusted_ceiling"), "{stdout}");
}

/// Runs a write to the worktree and to `/tmp` under `config` and returns
/// whether each landed: `(worktree, tmp)`. `None` when there is no confinement.
fn writes_under(config: &str, tag: &str) -> Option<(bool, bool)> {
    let base = outside_tmp()?;
    if !confinement_available() {
        return None;
    }
    let home = TempDir::new().unwrap();
    write_global_config(home.path(), config);
    let repo = base.path().join("repo");
    init_repo(home.path(), &repo);
    let tmp_file = unique_tmp_file(tag);
    let command = format!(
        "echo a > {repo}/x.txt; echo b > {tmp}",
        repo = repo.display(),
        tmp = tmp_file.display()
    );
    run_in(home.path(), &repo, &command);
    let landed = (repo.join("x.txt").exists(), tmp_file.exists());
    let _ = fs::remove_file(&tmp_file);
    Some(landed)
}

#[test]
fn an_explicit_empty_allow_write_leaves_nothing_writable() {
    let config = "[sandbox]\nenabled = true\nallow_write = []\n";
    if let Some(landed) = writes_under(config, "empty") {
        assert_eq!(landed, (false, false), "worktree, tmp");
    }
}

#[test]
fn an_allow_write_list_whose_entries_are_all_omitted_leaves_nothing_writable() {
    let config = "[sandbox]\nenabled = true\n\
                  allow_write = [\"relative/dir\", \"/var/tmp/aegis-no-such-dir-242\", \"/tmp/../var\"]\n";
    if let Some(landed) = writes_under(config, "omitted") {
        assert_eq!(landed, (false, false), "worktree, tmp");
    }
}

#[test]
fn ceiling_warnings_never_surface_in_the_shell_wrapper_path() {
    // A missing entry and a cwd outside any worktree each produce a warning
    // for `status` and `config validate`, never for `$SHELL -c`.
    let Some(base) = outside_tmp() else { return };
    if !confinement_available() {
        return;
    }
    let missing =
        "[sandbox]\nenabled = true\nallow_write = [\"/tmp\", \"/var/tmp/aegis-no-such-dir-242\"]\n";
    for config in [missing, ENABLED] {
        let home = TempDir::new().unwrap();
        write_global_config(home.path(), config);
        let cwd = base.path().join("plain");
        fs::create_dir_all(&cwd).unwrap();
        let output = run_in(home.path(), &cwd, "echo hi");
        let all = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(all.contains("hi"), "the command must have run: {all}");
        for code in ["trusted_ceiling_path_omitted", "workspace_unresolved"] {
            assert!(!all.contains(code), "{code} leaked into -c output: {all}");
        }
    }
}

#[test]
fn watch_uses_each_frame_cwd_and_defaults_to_process_cwd() {
    let Some(base) = outside_tmp() else { return };
    if !confinement_available() {
        return;
    }
    let home = hermetic_home();
    let a = base.path().join("a");
    let b = base.path().join("b");
    init_repo(home.path(), &a);
    init_repo(home.path(), &b);
    let frames = [
        serde_json::json!({"id": "b-own", "cwd": b, "cmd": format!("touch {}/b-own", b.display())}),
        serde_json::json!({"id": "b-other", "cwd": b, "cmd": format!("touch {}/b-other", a.display())}),
        serde_json::json!({"id": "default", "cmd": format!("touch {}/default", a.display())}),
    ];
    let mut child = base_command(home.path())
        .arg("watch")
        .env("AEGIS_FORCE_NO_TTY", "1")
        .current_dir(&a)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for frame in frames {
        writeln!(stdin, "{frame}").unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        b.join("b-own").exists(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(!a.join("b-other").exists());
    assert!(
        a.join("default").exists(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );

    write_global_config(home.path(), "[sandbox]\nallow_write = []\n");
    let mut child = base_command(home.path())
        .arg("watch")
        .env("AEGIS_FORCE_NO_TTY", "1")
        .current_dir(&a)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(
        child.stdin.take().unwrap(),
        "{}",
        serde_json::json!({
            "cwd": b, "cmd": format!("touch {}/restricted", b.display())
        })
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.lines().any(|line| {
            serde_json::from_str::<serde_json::Value>(line).is_ok_and(|frame| {
                frame["type"] == "result"
                    && frame["decision"] == "approved"
                    && frame["exit_code"] == 1
            })
        }),
        "{stdout}"
    );
    assert!(!b.join("restricted").exists(), "{stdout}");
}

#[test]
fn watch_runs_when_frame_cwd_cannot_be_canonicalized() {
    if !confinement_available() {
        return;
    }
    let home = hermetic_home();
    let base = TempDir::new().unwrap();
    let cwd = base.path().join("cwd");
    fs::create_dir(&cwd).unwrap();
    let mut child = base_command(home.path())
        .arg("watch")
        .env("AEGIS_FORCE_NO_TTY", "1")
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    writeln!(stdin, "{}", serde_json::json!({"id":"ready", "cmd":"true"})).unwrap();
    let mut line = String::new();
    loop {
        line.clear();
        assert!(stdout.read_line(&mut line).unwrap() > 0);
        let frame: serde_json::Value = serde_json::from_str(&line).unwrap();
        if frame["type"] == "result" && frame["id"] == "ready" {
            break;
        }
    }
    fs::remove_dir(&cwd).unwrap();
    let process_cwd = format!("/proc/{}/cwd", child.id());
    assert!(Path::new(&process_cwd).is_dir());
    assert!(fs::canonicalize(&process_cwd).is_err());
    writeln!(
        stdin,
        "{}",
        serde_json::json!({"id":"missing", "cwd":"/proc/self/cwd", "cmd":"true"})
    )
    .unwrap();
    drop(stdin);
    let mut frames = Vec::new();
    for line in stdout.lines() {
        frames.push(serde_json::from_str::<serde_json::Value>(&line.unwrap()).unwrap());
    }
    let status = child.wait().unwrap();
    assert!(status.success());
    assert!(
        frames.iter().any(|frame| frame["type"] == "result"
            && frame["id"] == "missing"
            && frame["exit_code"] == 0),
        "{frames:?}"
    );
}
