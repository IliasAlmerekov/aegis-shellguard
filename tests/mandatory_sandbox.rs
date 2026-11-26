mod support;

use std::io::Write;
use std::process::Stdio;

use tempfile::TempDir;

use support::{base_command, confinement_available, read_audit_entries, write_global_config};

#[test]
fn default_config_confines_shell_and_watch_with_active_sandbox_status() {
    if !confinement_available() {
        return;
    }
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();

    let shell_out = base_command(home.path())
        .current_dir(cwd.path())
        .args(["-c", "echo ok"])
        .output()
        .unwrap();
    assert!(
        shell_out.status.success(),
        "shell -c failed: {}",
        String::from_utf8_lossy(&shell_out.stderr)
    );

    let mut watch_child = base_command(home.path())
        .current_dir(cwd.path())
        .arg("watch")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    watch_child
        .stdin
        .take()
        .unwrap()
        .write_all(b"{\"cmd\":\"echo watch-ok\",\"id\":\"m1\"}\n")
        .unwrap();
    let watch_out = watch_child.wait_with_output().unwrap();
    assert!(
        watch_out.status.success(),
        "watch failed: {}",
        String::from_utf8_lossy(&watch_out.stderr)
    );

    let entries = read_audit_entries(home.path());
    assert_eq!(entries.len(), 2, "expected 2 audit entries: {entries:?}");
    for entry in &entries {
        assert_eq!(
            entry["sandbox_status"], "active",
            "expected SandboxStatus::Active, got: {entry:?}"
        );
    }
}

#[test]
fn audit_mode_runs_unconfined_with_not_configured_and_no_warning() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    write_global_config(home.path(), "mode = \"Audit\"\n");

    let output = base_command(home.path())
        .current_dir(cwd.path())
        .args(["-c", "echo audit-ok"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "audit mode -c failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("sandbox") && !stderr.contains("Sandbox"),
        "Audit mode must emit no per-command Sandbox warning, got: {stderr}"
    );

    let entries = read_audit_entries(home.path());
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["sandbox_status"], "not_configured");
}
