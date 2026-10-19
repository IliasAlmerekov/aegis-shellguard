//! The Snapshot ledger: `snapshot list`, `snapshot prune`, `rollback` and
//! automatic prune all read the Audit log the same way.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use aegis_audit::{AuditEntry, AuditLogger, AuditSnapshot};
use aegis_types::{Decision, RiskLevel};
use tempfile::TempDir;

fn aegis_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_aegis"))
}

fn default_audit_path(home: &Path) -> PathBuf {
    home.join(".aegis").join("audit.jsonl")
}

/// Run `aegis <args>` with `HOME` at `home` and the Audit log at `audit_path`.
fn run_aegis(home: &Path, audit_path: &Path, args: &[&str]) -> Output {
    Command::new(aegis_bin())
        .env("HOME", home)
        .env("AEGIS_AUDIT_PATH", audit_path)
        .env("AEGIS_CI", "0")
        .current_dir(home)
        .args(args)
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn snapshot_entry(
    command: &str,
    decision: Decision,
    plugin: &str,
    snapshot_id: &str,
) -> AuditEntry {
    AuditEntry::new(
        command,
        RiskLevel::Danger,
        Vec::new(),
        decision,
        vec![AuditSnapshot {
            plugin: plugin.to_string(),
            snapshot_id: snapshot_id.to_string(),
        }],
        None,
        None,
    )
}

fn record_creation(logger: &AuditLogger, plugin: &str, snapshot_id: &str) {
    logger
        .append(snapshot_entry(
            "rm -rf src",
            Decision::Approved,
            plugin,
            snapshot_id,
        ))
        .unwrap();
}

fn record_prune(logger: &AuditLogger, plugin: &str, snapshot_id: &str) {
    logger
        .append(snapshot_entry(
            &format!("aegis prune {snapshot_id}"),
            Decision::Pruned,
            plugin,
            snapshot_id,
        ))
        .unwrap();
}

/// Write a global config whose prune policy is `max_age_days = <days>`.
fn write_max_age_policy(home: &Path, days: u32) {
    let config_dir = home.join(".config/aegis");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("config.toml"),
        format!("[prune]\nenabled = true\nmax_age_days = {days}\n"),
    )
    .unwrap();
}

/// `(plugin, snapshot_id)` of every `Pruned` entry in the Audit log, all segments.
fn pruned_pairs(audit_path: &Path) -> Vec<(String, String)> {
    AuditLogger::new(audit_path)
        .read_all()
        .unwrap()
        .iter()
        .filter(|entry| entry.as_base().decision == Decision::Pruned)
        .flat_map(|entry| entry.as_base().snapshots.clone())
        .map(|snapshot| (snapshot.plugin, snapshot.snapshot_id))
        .collect()
}

#[test]
fn list_rollback_and_prune_agree_on_plugin_pairs() {
    let home = TempDir::new().unwrap();
    let audit_path = default_audit_path(home.path());
    let logger = AuditLogger::new(&audit_path);
    // `none` is the docker id of a Snapshot with no containers: restoring and
    // deleting it needs no docker daemon.
    let shared_id = "none";
    record_creation(&logger, "git", shared_id);
    record_creation(&logger, "docker", shared_id);
    record_prune(&logger, "git", shared_id);
    write_max_age_policy(home.path(), 0);

    let list = run_aegis(home.path(), &audit_path, &["snapshot", "list"]);
    let listing = stdout(&list);
    assert!(list.status.success(), "list failed: {}", stderr(&list));
    assert_eq!(listing.matches("id:       none").count(), 1, "{listing}");
    assert!(listing.contains("provider: docker"), "{listing}");
    assert!(!listing.contains("provider: git"), "{listing}");
    assert!(listing.contains("1 pruned snapshot(s) hidden"), "{listing}");

    let rollback = run_aegis(home.path(), &audit_path, &["rollback", shared_id]);
    assert!(
        rollback.status.success(),
        "rollback must restore docker:none: stderr={}",
        stderr(&rollback)
    );
    assert!(
        stdout(&rollback).contains("plugin=docker"),
        "rollback must pick docker: {}",
        stdout(&rollback)
    );

    let prune = run_aegis(home.path(), &audit_path, &["snapshot", "prune", "--yes"]);
    assert!(prune.status.success(), "prune failed: {}", stderr(&prune));
    assert_eq!(
        pruned_pairs(&audit_path),
        vec![
            ("git".to_string(), shared_id.to_string()),
            ("docker".to_string(), shared_id.to_string()),
        ],
        "prune must remove docker:none once and never prune git:none again"
    );
}

/// A git Snapshot id whose repository does not exist, so deleting it is a no-op.
fn missing_repo_git_id(home: &Path, hash: &str) -> String {
    format!("{}\t{hash}", home.join("missing-repo").display())
}

/// Append `entry` through a logger that rotates on every append, so whatever
/// the live log held moves to the gzip archive `<audit_path>.1.gz` first.
fn append_after_rotating_into_gzip_archive(audit_path: &Path, entry: AuditEntry) {
    let rotation = aegis_config::AuditConfig {
        rotation_enabled: true,
        max_file_size_bytes: 1,
        retention_files: 5,
        compress_rotated: true,
        ..aegis_config::AuditConfig::default()
    };
    AuditLogger::new(audit_path)
        .with_rotation(aegis_audit::AuditRotationPolicy::from_config(&rotation).unwrap())
        .append(entry)
        .unwrap();
}

#[test]
fn archived_prune_entry_hides_snapshot_under_custom_audit_path() {
    let home = TempDir::new().unwrap();
    let audit_path = home.path().join("custom-audit").join("trail.jsonl");
    let pruned_id = missing_repo_git_id(home.path(), "archived");
    let live_id = missing_repo_git_id(home.path(), "live");
    let logger = AuditLogger::new(&audit_path);
    record_creation(&logger, "git", &pruned_id);
    record_prune(&logger, "git", &pruned_id);
    append_after_rotating_into_gzip_archive(
        &audit_path,
        snapshot_entry("rm -rf src", Decision::Approved, "git", &live_id),
    );
    assert!(
        audit_path.with_file_name("trail.jsonl.1.gz").exists(),
        "the prune entry must sit in the gzip archive"
    );
    write_max_age_policy(home.path(), 0);

    let list = run_aegis(home.path(), &audit_path, &["snapshot", "list"]);
    let listing = stdout(&list);
    assert!(list.status.success(), "list failed: {}", stderr(&list));
    assert!(listing.contains(&live_id), "{listing}");
    assert!(!listing.contains(&pruned_id), "{listing}");
    assert!(listing.contains("1 pruned snapshot(s) hidden"), "{listing}");

    let rollback = run_aegis(home.path(), &audit_path, &["rollback", &pruned_id]);
    assert!(!rollback.status.success());
    assert!(
        stderr(&rollback).contains("pruned"),
        "rollback must say the Snapshot was pruned: {}",
        stderr(&rollback)
    );

    let prune = run_aegis(home.path(), &audit_path, &["snapshot", "prune", "--yes"]);
    assert!(prune.status.success(), "prune failed: {}", stderr(&prune));
    assert_eq!(
        pruned_pairs(&audit_path),
        vec![
            ("git".to_string(), pruned_id.clone()),
            ("git".to_string(), live_id.clone()),
        ],
        "prune must remove the live Snapshot and nothing for the archived prune"
    );
}

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        stderr(&output)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A git repository with one stash; returns the repository and the stash hash.
fn repo_with_stash(home: &Path) -> (PathBuf, String) {
    let repo = home.join("repo");
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
        &["stash", "push", "-q", "--include-untracked", "-m", "old"],
    );
    let hash = git(&repo, &["rev-parse", "stash@{0}"]);
    (repo, hash)
}

fn stash_hashes(repo: &Path) -> Vec<String> {
    git(repo, &["stash", "list", "--format=%H"])
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn unreadable_audit_line_fails_safe() {
    let home = TempDir::new().unwrap();
    let audit_path = default_audit_path(home.path());
    let (repo, hash) = repo_with_stash(home.path());
    let snapshot_id = format!("{}\t{hash}", repo.display());
    record_creation(&AuditLogger::new(&audit_path), "git", &snapshot_id);
    let mut log = fs::read_to_string(&audit_path).unwrap();
    log.push_str("not json\n");
    fs::write(&audit_path, log).unwrap();
    let config_dir = home.path().join(".config/aegis");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("config.toml"),
        "allowlist_override_level = \"Danger\"\n\n[prune]\nenabled = true\nmax_age_days = 0\n",
    )
    .unwrap();

    for args in [
        vec!["snapshot", "list"],
        vec!["snapshot", "prune", "--yes"],
        vec!["rollback", snapshot_id.as_str()],
    ] {
        let output = run_aegis(home.path(), &audit_path, &args);
        assert!(!output.status.success(), "{args:?} must fail");
        assert!(
            stderr(&output).contains("audit log"),
            "{args:?} must name the audit log: {}",
            stderr(&output)
        );
        assert_eq!(
            stash_hashes(&repo),
            vec![hash.clone()],
            "{args:?} touched the Snapshot"
        );
    }

    let target = "rm -rf /tmp/aegis-snapshot-ledger-target";
    fs::write(
        repo.join(".aegis.toml"),
        format!(
            "[[allow]]\npattern = \"{target}\"\ncwd = \"{}\"\nreason = \"snapshot ledger test\"\n",
            repo.display()
        ),
    )
    .unwrap();
    fs::write(repo.join("work.txt"), "work\n").unwrap();
    let through_aegis = Command::new(aegis_bin())
        .env("AEGIS_REAL_SHELL", "/bin/sh")
        .env("AEGIS_CI", "0")
        .env("HOME", home.path())
        .env("AEGIS_AUDIT_PATH", &audit_path)
        .current_dir(&repo)
        .args(["-c", target])
        .output()
        .unwrap();
    assert!(
        through_aegis.status.success(),
        "the command must still run: {}",
        stderr(&through_aegis)
    );
    assert!(
        stderr(&through_aegis).contains("automatic snapshot prune skipped"),
        "automatic prune must warn: {}",
        stderr(&through_aegis)
    );
    assert!(
        stash_hashes(&repo).contains(&hash),
        "automatic prune must delete nothing: {:?}",
        stash_hashes(&repo)
    );
}

#[test]
fn rollback_does_not_refresh_recorded_at() {
    let home = TempDir::new().unwrap();
    let audit_path = default_audit_path(home.path());
    let snapshot_id = missing_repo_git_id(home.path(), "ten-days-old");
    let ten_days_ago = (time::OffsetDateTime::now_utc() - time::Duration::days(10))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let creation = serde_json::json!({
        "timestamp": ten_days_ago,
        "sequence": 1,
        "command": "rm -rf src",
        "risk": "Danger",
        "matched_patterns": [],
        "pattern_ids": [],
        "decision": "Approved",
        "snapshots": [{ "plugin": "git", "snapshot_id": snapshot_id }],
    });
    fs::create_dir_all(audit_path.parent().unwrap()).unwrap();
    fs::write(&audit_path, format!("{creation}\n")).unwrap();
    // The entry `aegis rollback` writes after a restore: Safe, one Snapshot.
    AuditLogger::new(&audit_path)
        .append(AuditEntry::new(
            format!("aegis rollback {snapshot_id}"),
            RiskLevel::Safe,
            Vec::new(),
            Decision::Approved,
            vec![AuditSnapshot {
                plugin: "git".to_string(),
                snapshot_id: snapshot_id.clone(),
            }],
            None,
            None,
        ))
        .unwrap();
    write_max_age_policy(home.path(), 7);

    let prune = run_aegis(home.path(), &audit_path, &["snapshot", "prune", "--yes"]);

    assert!(prune.status.success(), "prune failed: {}", stderr(&prune));
    assert!(
        stdout(&prune).contains("Pruned 1 snapshot(s)"),
        "a rollback must not extend the Snapshot's life: {}",
        stdout(&prune)
    );
    assert_eq!(
        pruned_pairs(&audit_path),
        vec![("git".to_string(), snapshot_id)]
    );
}

#[test]
fn snapshot_list_survives_a_malformed_config() {
    let home = TempDir::new().unwrap();
    let audit_path = default_audit_path(home.path());
    record_creation(&AuditLogger::new(&audit_path), "docker", "none");
    let config_dir = home.path().join(".config/aegis");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(config_dir.join("config.toml"), "this is = = not toml [[\n").unwrap();

    let list = run_aegis(home.path(), &audit_path, &["snapshot", "list"]);

    assert!(list.status.success(), "list failed: {}", stderr(&list));
    assert!(
        stdout(&list).contains("id:       none"),
        "{}",
        stdout(&list)
    );
}
