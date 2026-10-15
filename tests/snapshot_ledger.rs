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
