//! Automatic Snapshot retention through the public command transports.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;
use tempfile::TempDir;

const TEST_COMMAND: &str = "rm -rf /tmp/aegis-automatic-prune-target";

fn aegis_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_aegis"))
}

fn init_git_repo(repo: &Path) {
    run_git(repo, &["init"]);
    run_git(repo, &["config", "user.email", "test@aegis.dev"]);
    run_git(repo, &["config", "user.name", "Aegis Test"]);
    run_git(repo, &["config", "core.autocrlf", "false"]);
    run_git(repo, &["commit", "--allow-empty", "-m", "init"]);
}

fn run_git(repo: &Path, args: &[&str]) -> Output {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap_or_else(|error| panic!("run git {args:?}: {error}"));
    assert!(
        output.status.success(),
        "git {args:?} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn create_stash(repo: &Path, name: &str) -> String {
    fs::write(repo.join(name), format!("{name}\n")).unwrap();
    run_git(repo, &["add", name]);
    run_git(repo, &["stash", "push", "--include-untracked", "-m", name]);

    let output = run_git(repo, &["rev-parse", "stash@{0}"]);
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn stash_hashes(repo: &Path) -> Vec<String> {
    let output = run_git(repo, &["stash", "list", "--format=%H"]);
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}

fn read_audit_entries(home: &Path) -> Vec<Value> {
    fs::read_to_string(home.join(".aegis/audit.jsonl"))
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn seed_old_snapshot(home: &Path, repo: &Path, hash: &str) -> String {
    let snapshot_id = format!("{}\t{hash}", repo.display());
    let log = home.join(".aegis/audit.jsonl");
    fs::create_dir_all(log.parent().unwrap()).unwrap();
    let entry = serde_json::json!({
        "timestamp": "2020-01-01T00:00:00Z",
        "sequence": 1,
        "command": "rm -rf src",
        "risk": "Danger",
        "matched_patterns": [],
        "pattern_ids": [],
        "decision": "Approved",
        "snapshots": [{ "plugin": "git", "snapshot_id": snapshot_id }],
    });
    fs::write(log, format!("{entry}\n")).unwrap();
    snapshot_id
}

fn write_config(home: &Path, repo: &Path, prune_enabled: bool, sandbox_enabled: bool) {
    let config_dir = home.join(".config/aegis");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("config.toml"),
        format!(
            "allowlist_override_level = \"Danger\"\n\n[prune]\nenabled = {prune_enabled}\nmax_count_per_provider = 1\n"
        ),
    )
    .unwrap();

    let sandbox = sandbox_enabled.then(|| {
        format!(
            "\n[sandbox]\nenabled = true\nrequired = false\nallow_write = [\"{}\"]\nallow_network = false\n",
            repo.display()
        )
    });
    fs::write(
        repo.join(".aegis.toml"),
        format!(
            "[[allow]]\npattern = \"rm -rf /tmp/aegis-automatic-prune-target\"\ncwd = \"{}\"\nreason = \"automatic prune integration test\"\n{}",
            repo.display(),
            sandbox.unwrap_or_default()
        ),
    )
    .unwrap();
}

fn run_shell(home: &Path, repo: &Path) -> Output {
    run_shell_command(home, repo, TEST_COMMAND)
}

fn run_shell_command(home: &Path, repo: &Path, command: &str) -> Output {
    Command::new(aegis_bin())
        .env("AEGIS_REAL_SHELL", "/bin/sh")
        .env("AEGIS_CI", "0")
        .env("HOME", home)
        .current_dir(repo)
        .args(["-c", command])
        .output()
        .unwrap()
}

fn run_watch(home: &Path, repo: &Path) -> Output {
    let mut child = Command::new(aegis_bin())
        .arg("watch")
        .env("AEGIS_REAL_SHELL", "/bin/sh")
        .env("AEGIS_CI", "0")
        .env("AEGIS_FORCE_NO_TTY", "1")
        .env("HOME", home)
        .current_dir(repo)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(
            format!("{{\"cmd\":\"{TEST_COMMAND}\",\"id\":\"automatic-prune\"}}\n").as_bytes(),
        )
        .unwrap();
    drop(child.stdin.take());
    child.wait_with_output().unwrap()
}

#[cfg(unix)]
fn sandbox_backend_available() -> bool {
    #[cfg(target_os = "linux")]
    {
        Command::new("bwrap")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
    #[cfg(target_os = "macos")]
    {
        const PROBE: &str = "(version 1)\n(deny default)\n(allow process*)\n(allow file-read*)\n";
        Command::new("/usr/bin/sandbox-exec")
            .args(["-p", PROBE, "/usr/bin/true"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        false
    }
}

fn assert_auto_pruned_old_snapshot(home: &Path, repo: &Path, old_hash: &str, old_id: &str) {
    let hashes = stash_hashes(repo);
    assert!(
        !hashes.iter().any(|hash| hash == old_hash),
        "automatic retention must remove the old Git Snapshot, found {hashes:?}"
    );
    assert_eq!(
        hashes.len(),
        1,
        "max_count_per_provider = 1 must retain one Git Snapshot, found {hashes:?}"
    );

    let entries = read_audit_entries(home);
    let pruned = entries
        .iter()
        .find(|entry| entry["decision"] == "Pruned")
        .expect("automatic retention must append a Pruned audit entry");
    assert_eq!(pruned["snapshots"][0]["plugin"], "git");
    assert_eq!(pruned["snapshots"][0]["snapshot_id"], old_id);
}

#[test]
fn disabled_prune_keeps_previous_git_snapshots() {
    let home = TempDir::new().unwrap();
    let repo = home.path().join("repo");
    fs::create_dir(&repo).unwrap();
    init_git_repo(&repo);
    let repo = repo.canonicalize().unwrap();

    let old_hash = create_stash(&repo, "aegis-snap-old");
    seed_old_snapshot(home.path(), &repo, &old_hash);
    fs::write(repo.join("current-work.txt"), "current\n").unwrap();
    write_config(home.path(), &repo, false, false);

    let output = run_shell(home.path(), &repo);
    assert!(
        output.status.success(),
        "approved shell command must succeed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let hashes = stash_hashes(&repo);
    assert_eq!(
        hashes.len(),
        2,
        "disabled automatic prune must retain both Git Snapshots, found {hashes:?}"
    );
    assert!(hashes.iter().any(|hash| hash == &old_hash));
    assert!(
        read_audit_entries(home.path())
            .iter()
            .all(|entry| entry["decision"] != "Pruned"),
        "disabled automatic prune must not append a Pruned audit entry"
    );
}

#[test]
fn audit_entries_without_a_snapshot_do_not_trigger_automatic_prune() {
    let home = TempDir::new().unwrap();
    let repo = home.path().join("repo");
    fs::create_dir(&repo).unwrap();
    init_git_repo(&repo);
    let repo = repo.canonicalize().unwrap();

    let old_hash = create_stash(&repo, "aegis-snap-old");
    seed_old_snapshot(home.path(), &repo, &old_hash);
    write_config(home.path(), &repo, true, false);

    let output = run_shell_command(home.path(), &repo, "printf automatic-prune");
    assert!(
        output.status.success(),
        "safe shell command must succeed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(stash_hashes(&repo), vec![old_hash]);
    assert!(
        read_audit_entries(home.path())
            .iter()
            .all(|entry| entry["decision"] != "Pruned"),
        "an audit entry without a Snapshot must not prune existing Snapshots"
    );
}

#[test]
fn shell_automatic_prune_retains_the_configured_git_snapshot_count() {
    let home = TempDir::new().unwrap();
    let repo = home.path().join("repo");
    fs::create_dir(&repo).unwrap();
    init_git_repo(&repo);
    let repo = repo.canonicalize().unwrap();

    let old_hash = create_stash(&repo, "aegis-snap-old");
    let old_id = seed_old_snapshot(home.path(), &repo, &old_hash);
    fs::write(repo.join("current-work.txt"), "current\n").unwrap();
    write_config(home.path(), &repo, true, false);

    let output = run_shell(home.path(), &repo);
    assert!(
        output.status.success(),
        "approved shell command must succeed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert_auto_pruned_old_snapshot(home.path(), &repo, &old_hash, &old_id);
}

#[test]
fn watch_automatic_prune_uses_the_same_git_retention_policy() {
    let home = TempDir::new().unwrap();
    let repo = home.path().join("repo");
    fs::create_dir(&repo).unwrap();
    init_git_repo(&repo);
    let repo = repo.canonicalize().unwrap();

    let old_hash = create_stash(&repo, "aegis-snap-old");
    let old_id = seed_old_snapshot(home.path(), &repo, &old_hash);
    fs::write(repo.join("current-work.txt"), "current\n").unwrap();
    write_config(home.path(), &repo, true, false);

    let output = run_watch(home.path(), &repo);
    assert!(
        output.status.success(),
        "approved watch command must succeed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert_auto_pruned_old_snapshot(home.path(), &repo, &old_hash, &old_id);
}

#[cfg(unix)]
#[test]
fn sandboxed_shell_automatic_prune_uses_the_same_git_retention_policy() {
    if !sandbox_backend_available() {
        println!("sandbox backend not available on this host; skipping");
        return;
    }

    let home = TempDir::new().unwrap();
    let repo = home.path().join("repo");
    fs::create_dir(&repo).unwrap();
    init_git_repo(&repo);
    let repo = repo.canonicalize().unwrap();

    let old_hash = create_stash(&repo, "aegis-snap-old");
    let old_id = seed_old_snapshot(home.path(), &repo, &old_hash);
    fs::write(repo.join("current-work.txt"), "current\n").unwrap();
    write_config(home.path(), &repo, true, true);

    let output = Command::new(aegis_bin())
        .env("AEGIS_REAL_SHELL", "/bin/sh")
        .env("AEGIS_CI", "0")
        .env("HOME", home.path())
        .current_dir(&repo)
        .args(["-c", TEST_COMMAND])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success()
        && stderr.contains("landlock restrict_self: Operation not permitted")
    {
        println!("landlock restrict_self unavailable in this environment; skipping");
        return;
    }
    assert!(
        output.status.success(),
        "sandboxed command must succeed:\n{stderr}"
    );

    assert_auto_pruned_old_snapshot(home.path(), &repo, &old_hash, &old_id);
}
