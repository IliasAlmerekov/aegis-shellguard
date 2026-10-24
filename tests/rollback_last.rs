//! `aegis rollback --last` (#499, ADR-053): the latest command whose Snapshots
//! are bound to the current location, restored through the #215 preflight.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use aegis_audit::{AuditEntry, AuditLogger, AuditSnapshot};
use aegis_types::{Decision, RiskLevel};
use tempfile::TempDir;

const DOCKER_RECORD: &str = r#"{"container_id":"c1","image":"img:1","config":{"name":"web","binds":[],"port_bindings":[],"labels":{},"network_mode":"default","restart_policy":"no"}}"#;

struct Env {
    home: TempDir,
}

/// One Snapshot to record: plugin, id, git target.
type Rec<'a> = (&'a str, String, Option<&'a Path>);

impl Env {
    fn new() -> Self {
        Self {
            home: TempDir::new().unwrap(),
        }
    }

    fn audit_path(&self) -> PathBuf {
        self.home.path().join(".aegis").join("audit.jsonl")
    }

    /// Record a command that ran in `cwd` (or in an unknown place) with `snaps`.
    fn record(&self, command: &str, cwd: Option<&Path>, snaps: Vec<Rec<'_>>) {
        let snapshots = snaps
            .into_iter()
            .map(|(plugin, snapshot_id, target)| AuditSnapshot {
                plugin: plugin.to_string(),
                snapshot_id,
                git_target: target.map(|t| t.display().to_string()),
            })
            .collect();
        let entry = AuditEntry::new(
            command,
            RiskLevel::Danger,
            Vec::new(),
            Decision::Approved,
            snapshots,
            None,
            None,
        )
        .with_cwd(cwd.map(|c| c.display().to_string()));
        AuditLogger::new(self.audit_path()).append(entry).unwrap();
    }

    /// Run `aegis <args>` in `cwd`; stdin is null, so there is never a terminal.
    fn run(&self, cwd: &Path, args: &[&str], bin_dir: Option<&Path>) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_aegis"));
        command
            .env("HOME", self.home.path())
            .env("AEGIS_AUDIT_PATH", self.audit_path())
            .env("AEGIS_CI", "0")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .current_dir(cwd)
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

    /// A git repository with one commit, under the temp home.
    fn repo(&self, name: &str) -> PathBuf {
        let repo = self.home.path().join(name);
        fs::create_dir(&repo).unwrap();
        let repo = repo.canonicalize().unwrap();
        git(&repo, &["init", "-q"]);
        git(&repo, &["config", "user.email", "test@aegis.dev"]);
        git(&repo, &["config", "user.name", "Aegis Test"]);
        git(&repo, &["config", "core.autocrlf", "false"]);
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "init"]);
        repo
    }

    /// Configure the sqlite provider for `repo` and keep its files out of the
    /// git stash: a git restore parks untracked files, which would pull
    /// `app.db` from under the sqlite restore.
    fn use_sqlite_in(&self, repo: &Path) {
        fs::write(
            repo.join(".aegis.toml"),
            "auto_snapshot_sqlite = true\nsqlite_snapshot_path = \"app.db\"\n",
        )
        .unwrap();
        fs::write(repo.join(".gitignore"), "app.db\n.aegis.toml\n").unwrap();
        git(repo, &["add", ".gitignore"]);
        git(repo, &["commit", "-q", "-m", "ignore sqlite files"]);
    }

    /// Take a git Snapshot the way the plugin does (stash, then put the work
    /// back), then run "the command": delete the file. Returns the Snapshot id.
    fn snapshot_then_delete(&self, repo: &Path, file: &str) -> String {
        fs::write(repo.join(file), "precious\n").unwrap();
        git(
            repo,
            &["stash", "push", "-q", "--include-untracked", "-m", "snap"],
        );
        let hash = git(repo, &["rev-parse", "stash@{0}"]);
        git(repo, &["stash", "apply", "-q", "--index", &hash]);
        fs::remove_file(repo.join(file)).unwrap();
        format!("{}\t{hash}", repo.display())
    }

    /// A fake `docker` that logs every call; returns its bin dir and the log.
    fn fake_docker(&self) -> (PathBuf, PathBuf) {
        let bin_dir = self.home.path().join("fake-bin");
        fs::create_dir(&bin_dir).unwrap();
        let log = self.home.path().join("docker.log");
        let script = bin_dir.join("docker");
        fs::write(
            &script,
            format!("#!/bin/sh\necho \"$@\" >> '{}'\nexit 0\n", log.display()),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        (bin_dir, log)
    }

    fn rollback_entries(&self) -> Vec<String> {
        fs::read_to_string(self.audit_path())
            .unwrap()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter_map(|v| v["command"].as_str().map(str::to_string))
            .filter(|command| command.starts_with("aegis rollback "))
            .collect()
    }
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
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
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

#[test]
fn two_agents_last_restores_repo_a_and_never_the_container() {
    let env = Env::new();
    let repo_a = env.repo("repo-a");
    let project_b = env.repo("project-b");
    let git_id = env.snapshot_then_delete(&repo_a, "plans.txt");
    env.record(
        "rm -rf plans",
        Some(&repo_a),
        vec![("git", git_id, Some(&repo_a))],
    );
    env.record(
        "docker rm web",
        Some(&project_b),
        vec![("docker", DOCKER_RECORD.to_string(), None)],
    );
    let (bin_dir, docker_log) = env.fake_docker();

    let output = env.run(&repo_a, &["rollback", "--last", "--yes"], Some(&bin_dir));

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(
        fs::read_to_string(repo_a.join("plans.txt")).unwrap(),
        "precious\n"
    );
    assert!(
        !docker_log.exists(),
        "docker must not be touched: {}",
        fs::read_to_string(&docker_log).unwrap_or_default()
    );
}

#[test]
fn multi_provider_last_restores_git_and_sqlite_and_prints_the_docker_line() {
    let env = Env::new();
    let repo = env.repo("repo");
    env.use_sqlite_in(&repo);
    let git_id = env.snapshot_then_delete(&repo, "notes.txt");
    let store = env.home.path().join(".aegis").join("snapshots");
    fs::create_dir_all(&store).unwrap();
    let dump = store.canonicalize().unwrap().join("app-1.db");
    fs::write(&dump, "before").unwrap();
    fs::write(repo.join("app.db"), "after").unwrap();
    let hex = |p: &Path| {
        p.to_string_lossy()
            .bytes()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    let sqlite_id = format!("v2\t{}\t{}", hex(&repo.join("app.db")), hex(&dump));
    env.record(
        "terraform destroy",
        Some(&repo),
        vec![
            ("git", git_id, Some(&repo)),
            ("sqlite", sqlite_id, None),
            ("docker", DOCKER_RECORD.to_string(), None),
        ],
    );
    let (bin_dir, docker_log) = env.fake_docker();

    let output = env.run(&repo, &["rollback", "--last", "--yes"], Some(&bin_dir));

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(
        fs::read_to_string(repo.join("notes.txt")).unwrap(),
        "precious\n"
    );
    assert_eq!(fs::read_to_string(repo.join("app.db")).unwrap(), "before");
    assert!(!docker_log.exists());
    assert!(
        stdout(&output).contains(&format!("aegis rollback {DOCKER_RECORD}")),
        "{}",
        stdout(&output)
    );
    let verify = env.run(&repo, &["audit", "--verify-integrity"], None);
    assert_eq!(code(&verify), 0, "{}{}", stdout(&verify), stderr(&verify));
}

#[test]
fn two_candidates_without_a_terminal_are_listed_and_nothing_is_restored() {
    let env = Env::new();
    let repo = env.repo("repo");
    let older = env.snapshot_then_delete(&repo, "older.txt");
    env.record("rm older", Some(&repo), vec![("git", older, Some(&repo))]);
    let newer = env.snapshot_then_delete(&repo, "newer.txt");
    env.record("rm newer", Some(&repo), vec![("git", newer, Some(&repo))]);

    let output = env.run(&repo, &["rollback", "--last"], None);

    assert_eq!(code(&output), 2, "{}", stderr(&output));
    let text = stderr(&output);
    assert!(
        text.contains("rm older") && text.contains("rm newer"),
        "{text}"
    );
    assert!(text.contains("--yes"), "{text}");
    assert!(!repo.join("older.txt").exists() && !repo.join("newer.txt").exists());
}

#[test]
fn last_with_yes_restores_the_latest_of_two_candidates() {
    let env = Env::new();
    let repo = env.repo("repo");
    let older = env.snapshot_then_delete(&repo, "older.txt");
    env.record("rm older", Some(&repo), vec![("git", older, Some(&repo))]);
    let newer = env.snapshot_then_delete(&repo, "newer.txt");
    env.record("rm newer", Some(&repo), vec![("git", newer, Some(&repo))]);

    let output = env.run(&repo, &["rollback", "--last", "--yes"], None);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(repo.join("newer.txt").exists());
    assert!(!repo.join("older.txt").exists());
}

#[test]
fn last_without_a_candidate_exits_non_zero_naming_the_location() {
    let env = Env::new();
    let repo = env.repo("repo");
    env.record(
        "docker rm web",
        Some(&repo),
        vec![("docker", DOCKER_RECORD.to_string(), None)],
    );

    let output = env.run(&repo, &["rollback", "--last", "--yes"], None);

    assert_eq!(code(&output), 4, "{}", stderr(&output));
    let text = stderr(&output);
    assert!(text.contains(&repo.display().to_string()), "{text}");
    assert!(text.contains("aegis rollback <snapshot_id>"), "{text}");
    assert!(text.contains("aegis audit"), "{text}");
}

#[test]
fn legacy_entry_without_cwd_is_skipped_by_last_but_restores_by_id() {
    let env = Env::new();
    let repo = env.repo("repo");
    let git_id = env.snapshot_then_delete(&repo, "legacy.txt");
    env.record("rm legacy", None, vec![("git", git_id.clone(), None)]);

    let last = env.run(&repo, &["rollback", "--last", "--yes"], None);
    assert_eq!(code(&last), 4, "{}", stderr(&last));
    assert!(!repo.join("legacy.txt").exists());

    let by_id = env.run(&repo, &["rollback", &git_id], None);
    assert_eq!(code(&by_id), 0, "{}", stderr(&by_id));
    assert!(repo.join("legacy.txt").exists());
}

#[test]
fn a_failing_provider_does_not_stop_the_others_and_gets_no_audit_entry() {
    let env = Env::new();
    let repo = env.repo("repo");
    env.use_sqlite_in(&repo);
    let git_id = env.snapshot_then_delete(&repo, "kept.txt");
    let store = env.home.path().join(".aegis").join("snapshots");
    fs::create_dir_all(&store).unwrap();
    let hex = |p: &Path| {
        p.to_string_lossy()
            .bytes()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    let missing_dump = store.canonicalize().unwrap().join("gone.db");
    let sqlite_id = format!("v2\t{}\t{}", hex(&repo.join("app.db")), hex(&missing_dump));
    // The failing provider comes first, so a stop-on-error loop would never reach git.
    env.record(
        "terraform destroy",
        Some(&repo),
        vec![
            ("sqlite", sqlite_id.clone(), None),
            ("git", git_id.clone(), Some(&repo)),
        ],
    );
    fs::write(repo.join("app.db"), "after").unwrap();

    let output = env.run(&repo, &["rollback", "--last", "--yes"], None);

    assert_eq!(code(&output), 1, "{}", stderr(&output));
    assert!(repo.join("kept.txt").exists(), "git must still be restored");
    assert_eq!(fs::read_to_string(repo.join("app.db")).unwrap(), "after");
    assert!(stderr(&output).contains("sqlite"), "{}", stderr(&output));
    assert_eq!(
        env.rollback_entries(),
        vec![format!("aegis rollback {git_id}")]
    );
    assert!(
        !env.rollback_entries()
            .iter()
            .any(|c| c.contains(&sqlite_id))
    );
}

#[test]
fn last_still_runs_the_preflight_and_refuses_an_absent_snapshot() {
    let env = Env::new();
    let repo = env.repo("repo");
    let git_id = env.snapshot_then_delete(&repo, "gone.txt");
    env.record("rm gone", Some(&repo), vec![("git", git_id, Some(&repo))]);
    git(&repo, &["stash", "drop", "-q"]);

    let output = env.run(&repo, &["rollback", "--last", "--yes"], None);

    assert_eq!(code(&output), 1, "{}", stderr(&output));
    assert!(!repo.join("gone.txt").exists());
    assert!(env.rollback_entries().is_empty());
}

#[test]
fn snapshot_id_and_last_conflict() {
    let env = Env::new();

    let output = env.run(env.home.path(), &["rollback", "--last", "some-id"], None);

    assert_ne!(code(&output), 0);
    let neither = env.run(env.home.path(), &["rollback"], None);
    assert_ne!(code(&neither), 0);
}
