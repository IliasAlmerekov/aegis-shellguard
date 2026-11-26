mod support;

use std::fs;
use std::path::Path;

use aegis_audit::AuditLogger;
use aegis_types::Decision;
use support::{run_prune, seed_snapshot, snapshot_list_command};
use tempfile::TempDir;

fn write_prune_policy(home: &Path) {
    let config_dir = home.join(".config/aegis");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("config.toml"),
        "[prune]\nenabled = true\nmax_age_days = 0\n",
    )
    .unwrap();
}

fn git_snapshot_id(home: &Path, name: &str) -> String {
    format!("{}\t{name}", home.join("nonexistent-repo").display())
}

fn last_audit_decision(home: &Path) -> Decision {
    let logger = AuditLogger::new(home.join(".aegis").join("audit.jsonl"));
    let entries = logger.read_all().unwrap();
    entries
        .last()
        .expect("audit log must contain entries")
        .as_base()
        .decision
}

fn append_raw_pruned_entry(home: &Path, snapshot_id: &str) {
    use std::fs::OpenOptions;
    use std::io::Write;

    let log = home.join(".aegis").join("audit.jsonl");
    fs::create_dir_all(log.parent().unwrap()).unwrap();
    let command = format!("aegis prune {}", snapshot_id);
    let escaped_command = serde_json::to_string(&command).unwrap();
    let line = format!(
        "{{\"timestamp\":\"2026-01-02T00:00:00Z\",\"sequence\":2,\"command\":{},\"risk\":\"Safe\",\"matched_patterns\":[],\"pattern_ids\":[],\"decision\":\"Pruned\",\"snapshots\":[],\"sandbox_status\":\"not_configured\"}}\n",
        escaped_command
    );
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .unwrap();
    file.write_all(line.as_bytes()).unwrap();
}

#[test]
fn test_prune_default_shows_dry_run_preview() {
    let home = TempDir::new().unwrap();
    write_prune_policy(home.path());
    let snapshot_id = git_snapshot_id(home.path(), "deadbeef");
    seed_snapshot(home.path(), "git", &snapshot_id);

    let output = run_prune(home.path(), &[]);

    assert!(
        output.status.success(),
        "default prune must exit 0: stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.to_lowercase().contains("would prune"),
        "default prune must be a dry-run preview: stdout=\n{stdout}"
    );
    assert!(
        stdout.contains(&snapshot_id),
        "default prune must name the candidate id: stdout=\n{stdout}"
    );
}

#[test]
fn test_prune_yes_ignores_hostile_project_retention() {
    let home = TempDir::new().unwrap();
    fs::write(
        home.path().join(".aegis.toml"),
        "[prune]\nenabled = true\nmax_count_per_provider = 0\nmax_age_days = 0\n",
    )
    .unwrap();
    let snapshot_id = git_snapshot_id(home.path(), "deadbeef");
    seed_snapshot(home.path(), "git", &snapshot_id);

    let output = run_prune(home.path(), &["--yes"]);

    assert!(
        output.status.success(),
        "prune --yes must exit 0: stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let list_output = snapshot_list_command(home.path())
        .current_dir(home.path())
        .output()
        .unwrap();
    let list_stdout = String::from_utf8_lossy(&list_output.stdout);
    assert!(
        list_stdout.contains(&snapshot_id),
        "a project config must not prune the snapshot: stdout=\n{list_stdout}"
    );
    assert_ne!(
        last_audit_decision(home.path()),
        Decision::Pruned,
        "audit log must not gain a Pruned entry"
    );
}

#[test]
fn test_prune_yes_flag_executes_deletion() {
    let home = TempDir::new().unwrap();
    write_prune_policy(home.path());
    let snapshot_id = git_snapshot_id(home.path(), "deadbeef");
    seed_snapshot(home.path(), "git", &snapshot_id);

    let output = run_prune(home.path(), &["--yes"]);

    assert!(
        output.status.success(),
        "prune --yes must execute: stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let list_output = snapshot_list_command(home.path())
        .current_dir(home.path())
        .output()
        .unwrap();
    let list_stdout = String::from_utf8_lossy(&list_output.stdout);
    assert!(
        !list_stdout.contains(&snapshot_id),
        "snapshot list must hide pruned id: stdout=\n{list_stdout}"
    );

    assert_eq!(
        last_audit_decision(home.path()),
        Decision::Pruned,
        "audit log must end with a Pruned entry"
    );
}

#[test]
fn test_prune_dry_run_and_yes_conflict() {
    let home = TempDir::new().unwrap();
    write_prune_policy(home.path());
    seed_snapshot(
        home.path(),
        "git",
        &git_snapshot_id(home.path(), "deadbeef"),
    );

    let output = run_prune(home.path(), &["--dry-run", "--yes"]);

    assert!(
        !output.status.success(),
        "--dry-run with --yes must fail: stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
    assert!(
        stderr.contains("conflict") || stderr.contains("cannot"),
        "error must explain the flag conflict: stderr=\n{stderr}"
    );
}

#[test]
fn test_prune_yes_reports_delete_failure_as_error() {
    let home = TempDir::new().unwrap();
    write_prune_policy(home.path());
    let snapshot_id = "not-a-tab-separator";
    seed_snapshot(home.path(), "git", snapshot_id);

    let output = run_prune(home.path(), &["--yes"]);

    assert!(
        !output.status.success(),
        "prune --yes must fail when a backend delete fails: stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
    assert!(
        stderr.contains("malformed") || stderr.contains("prune failed"),
        "stderr must surface the delete failure: stderr=\n{stderr}"
    );
}

#[test]
fn test_prune_skips_already_pruned_snapshot_in_command_field() {
    let home = TempDir::new().unwrap();
    write_prune_policy(home.path());
    let snapshot_id = git_snapshot_id(home.path(), "deadbeef");
    seed_snapshot(home.path(), "git", &snapshot_id);
    append_raw_pruned_entry(home.path(), &snapshot_id);

    let output = run_prune(home.path(), &["--yes"]);

    assert!(
        output.status.success(),
        "prune --yes must succeed: stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let logger = AuditLogger::new(home.path().join(".aegis").join("audit.jsonl"));
    let pruned_entries: Vec<_> = logger
        .read_all()
        .unwrap()
        .into_iter()
        .filter(|e| e.as_base().decision == Decision::Pruned)
        .collect();
    assert_eq!(
        pruned_entries.len(),
        1,
        "a snapshot already recorded as pruned in the command field must not be re-pruned: {pruned_entries:?}"
    );
}
