//! `aegis snapshot verify` and the `aegis rollback` liveness preflight (#215, ADR-052).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use aegis_audit::{AuditEntry, AuditLogger, AuditSnapshot};
use aegis_types::{Decision, RiskLevel};
use tempfile::TempDir;

fn aegis_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_aegis"))
}

struct Env {
    home: TempDir,
}

impl Env {
    fn new() -> Self {
        Self {
            home: TempDir::new().unwrap(),
        }
    }

    fn audit_path(&self) -> PathBuf {
        self.home.path().join(".aegis").join("audit.jsonl")
    }

    fn record(&self, plugin: &str, snapshot_id: &str) {
        AuditLogger::new(self.audit_path())
            .append(AuditEntry::new(
                "rm -rf src",
                RiskLevel::Danger,
                Vec::new(),
                Decision::Approved,
                vec![AuditSnapshot {
                    plugin: plugin.to_string(),
                    snapshot_id: snapshot_id.to_string(),
                }],
                None,
                None,
            ))
            .unwrap();
    }

    /// Run `aegis <args>`; stdin is null, so there is never a terminal.
    fn run(&self, args: &[&str]) -> Output {
        self.run_with_path(args, None)
    }

    fn run_with_path(&self, args: &[&str], bin_dir: Option<&Path>) -> Output {
        let mut command = Command::new(aegis_bin());
        command
            .env("HOME", self.home.path())
            .env("AEGIS_AUDIT_PATH", self.audit_path())
            .env("AEGIS_CI", "0")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .current_dir(self.home.path())
            .stdin(Stdio::null())
            .args(args);
        if let Some(dir) = bin_dir {
            let path = std::env::var_os("PATH").unwrap_or_default();
            let mut paths = vec![dir.to_path_buf()];
            paths.extend(std::env::split_paths(&path));
            command.env("PATH", std::env::join_paths(paths).unwrap());
        }
        command.output().unwrap()
    }

    fn audit_len(&self) -> usize {
        fs::read_to_string(self.audit_path()).unwrap().len()
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn code(output: &Output) -> i32 {
    output.status.code().unwrap()
}

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {}", stderr(&output));
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A git repository with one stash; returns the repository and the git Snapshot id.
fn repo_with_stash(env: &Env) -> (PathBuf, String) {
    let repo = env.home.path().join("repo");
    fs::create_dir(&repo).unwrap();
    let repo = repo.canonicalize().unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.email", "test@aegis.dev"]);
    git(&repo, &["config", "user.name", "Aegis Test"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "init"]);
    fs::write(repo.join("stashed.txt"), "stashed\n").unwrap();
    git(
        &repo,
        &["stash", "push", "-q", "--include-untracked", "-m", "snap"],
    );
    let hash = git(&repo, &["rev-parse", "stash@{0}"]);
    let id = format!("{}\t{hash}", repo.display());
    (repo, id)
}

fn stash_list(repo: &Path) -> String {
    git(repo, &["stash", "list", "--format=%H"])
}

/// A fake `docker` whose `image inspect` fails the way a dead daemon does.
/// Every call is appended to the returned log.
fn fake_dead_docker(env: &Env) -> (PathBuf, PathBuf) {
    let bin_dir = env.home.path().join("fake-bin");
    fs::create_dir(&bin_dir).unwrap();
    let log = env.home.path().join("docker.log");
    let script = bin_dir.join("docker");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\necho \"$@\" >> '{}'\nif [ \"$1\" = image ]; then\n  echo 'Cannot connect to the Docker daemon' >&2\n  exit 1\nfi\nexit 0\n",
            log.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    (bin_dir, log)
}

const DOCKER_RECORD: &str = r#"{"container_id":"c1","image":"img:1","config":{"name":"web","binds":[],"port_bindings":[],"labels":{},"network_mode":"default","restart_policy":"no"}}"#;

#[test]
fn verify_exits_0_when_every_snapshot_is_present_or_nothing_captured() {
    let env = Env::new();
    let (_repo, id) = repo_with_stash(&env);
    env.record("git", &id);
    env.record("docker", "none");

    let output = env.run(&["snapshot", "verify"]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(stdout(&output).contains("present"), "{}", stdout(&output));
    assert!(stdout(&output).contains("nothing_captured"));
}

#[test]
fn verify_exits_1_when_a_stash_entry_was_dropped() {
    let env = Env::new();
    let (repo, id) = repo_with_stash(&env);
    env.record("git", &id);
    git(&repo, &["stash", "drop", "-q"]);

    let output = env.run(&["snapshot", "verify"]);

    assert_eq!(code(&output), 1, "{}", stderr(&output));
    assert!(stdout(&output).contains("absent"), "{}", stdout(&output));
}

#[test]
fn verify_exits_5_when_the_docker_daemon_does_not_answer() {
    let env = Env::new();
    let (bin_dir, _log) = fake_dead_docker(&env);
    env.record("docker", DOCKER_RECORD);

    let output = env.run_with_path(&["snapshot", "verify"], Some(&bin_dir));

    assert_eq!(code(&output), 5, "{}", stderr(&output));
    assert!(stdout(&output).contains("inconclusive"));
}

#[test]
fn verify_exits_4_for_an_unknown_id() {
    let env = Env::new();
    env.record("docker", "none");

    let output = env.run(&["snapshot", "verify", "no-such-id"]);

    assert_eq!(code(&output), 4, "{}", stderr(&output));
}

#[test]
fn verify_exits_4_and_shows_the_reason_for_a_malformed_record() {
    let env = Env::new();
    env.record("docker", "not json");

    let output = env.run(&["snapshot", "verify"]);

    assert_eq!(code(&output), 4);
    assert!(stdout(&output).contains("error"), "{}", stdout(&output));
    assert!(stdout(&output).contains("malformed"), "{}", stdout(&output));
}

#[test]
fn verify_with_an_id_checks_only_that_snapshot() {
    let env = Env::new();
    let (repo, id) = repo_with_stash(&env);
    env.record("git", &id);
    env.record("docker", "none");
    git(&repo, &["stash", "drop", "-q"]);

    let output = env.run(&["snapshot", "verify", "none"]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(!stdout(&output).contains("absent"));
}

#[test]
fn verify_json_carries_the_same_outcomes_as_text() {
    let env = Env::new();
    let (repo, id) = repo_with_stash(&env);
    env.record("git", &id);
    env.record("docker", "none");
    git(&repo, &["stash", "drop", "-q"]);

    let text = env.run(&["snapshot", "verify"]);
    let json = env.run(&["snapshot", "verify", "--format", "json"]);

    assert_eq!(code(&json), code(&text));
    let rows: serde_json::Value = serde_json::from_str(&stdout(&json)).unwrap();
    let outcomes: Vec<(&str, &str)> = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["plugin"].as_str().unwrap(),
                row["outcome"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        outcomes,
        [("git", "absent"), ("docker", "nothing_captured")]
    );
    for (plugin, outcome) in outcomes {
        let line = stdout(&text)
            .lines()
            .find(|line| line.starts_with(plugin))
            .unwrap()
            .to_string();
        assert!(line.contains(outcome), "{line}");
    }
}

#[test]
fn rollback_refuses_an_absent_snapshot_with_exit_1_and_changes_nothing() {
    let env = Env::new();
    let (repo, id) = repo_with_stash(&env);
    env.record("git", &id);
    git(&repo, &["stash", "drop", "-q"]);
    fs::write(repo.join("work.txt"), "work\n").unwrap();
    let audit_before = env.audit_len();

    for args in [
        vec!["rollback", id.as_str()],
        vec!["rollback", id.as_str(), "--yes"],
    ] {
        let output = env.run(&args);
        assert_eq!(code(&output), 1, "{args:?}: {}", stderr(&output));
        assert!(
            stderr(&output).contains("confirmed gone"),
            "{}",
            stderr(&output)
        );
    }

    assert_eq!(stash_list(&repo), "");
    assert_eq!(fs::read_to_string(repo.join("work.txt")).unwrap(), "work\n");
    assert!(!repo.join("stashed.txt").exists());
    assert_eq!(env.audit_len(), audit_before);
}

#[test]
fn rollback_of_an_inconclusive_snapshot_without_a_terminal_exits_2_and_names_yes() {
    let env = Env::new();
    let (bin_dir, log) = fake_dead_docker(&env);
    env.record("docker", DOCKER_RECORD);
    let audit_before = env.audit_len();

    let output = env.run_with_path(&["rollback", DOCKER_RECORD], Some(&bin_dir));

    assert_eq!(code(&output), 2, "{}", stderr(&output));
    assert!(stderr(&output).contains("--yes"), "{}", stderr(&output));
    assert!(!fs::read_to_string(&log).unwrap().contains("stop"));
    assert_eq!(env.audit_len(), audit_before);
}

#[test]
fn rollback_yes_attempts_the_restore_of_an_inconclusive_snapshot() {
    let env = Env::new();
    let (bin_dir, log) = fake_dead_docker(&env);
    env.record("docker", DOCKER_RECORD);

    let output = env.run_with_path(&["rollback", DOCKER_RECORD, "--yes"], Some(&bin_dir));

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let calls = fs::read_to_string(&log).unwrap();
    assert!(
        calls.contains("stop c1") && calls.contains("run"),
        "{calls}"
    );
}

#[test]
fn rollback_restores_a_present_snapshot_without_a_terminal_or_prompt() {
    let env = Env::new();
    let (repo, id) = repo_with_stash(&env);
    env.record("git", &id);

    let output = env.run(&["rollback", &id]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(
        fs::read_to_string(repo.join("stashed.txt")).unwrap(),
        "stashed\n"
    );
}

#[test]
fn rollback_exits_4_without_asking_when_the_liveness_check_errors() {
    let env = Env::new();
    env.record("docker", "not json");

    let output = env.run(&["rollback", "not json", "--yes"]);

    assert_eq!(code(&output), 4, "{}", stderr(&output));
}
