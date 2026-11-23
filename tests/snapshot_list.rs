mod support;

use support::{run_snapshot_list, seed_snapshot};
use tempfile::TempDir;

#[test]
fn snapshot_list_prints_recorded_snapshot() {
    let home = TempDir::new().unwrap();
    seed_snapshot(home.path(), "git", "snap-git-abc123");

    let output = run_snapshot_list(home.path());

    assert!(
        output.status.success(),
        "snapshot list failed: stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("snap-git-abc123"), "stdout=\n{stdout}");
    assert!(stdout.contains("git"), "stdout=\n{stdout}");
}

#[test]
fn snapshot_list_on_empty_log_exits_zero_with_message() {
    let home = TempDir::new().unwrap();

    let output = run_snapshot_list(home.path());

    assert!(
        output.status.success(),
        "snapshot list must not error on an empty log: stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout).to_lowercase();
    assert!(stdout.contains("no snapshot"), "stdout=\n{stdout}");
}
