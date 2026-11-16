//! End-to-end tests for mandatory Sandbox confinement (#229, ADR-029).

mod support;

use std::fs;
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

    // Shell `-c` with default config (no config file at all) must confine
    // and record SandboxStatus::Active in audit.jsonl.
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

    // Watch mode with default config must also confine and record Active.
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

#[test]
fn deprecated_sandbox_fields_warn_only_in_validate_and_status_and_preserve_files() {
    if !confinement_available() {
        return;
    }
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let global_body = "[sandbox]\nenabled = false\nrequired = false\n";
    let project_body = "[sandbox]\nenabled = false\nrequired = false\n";
    write_global_config(home.path(), global_body);
    let global_path = home.path().join(".config/aegis/config.toml");
    let project_path = cwd.path().join(".aegis.toml");
    fs::write(&project_path, project_body).unwrap();

    let validate = base_command(home.path())
        .current_dir(cwd.path())
        .args(["config", "validate"])
        .output()
        .unwrap();
    assert!(
        validate.status.success(),
        "config validate failed: {}",
        String::from_utf8_lossy(&validate.stderr)
    );
    let validate_out = format!(
        "{}{}",
        String::from_utf8_lossy(&validate.stdout),
        String::from_utf8_lossy(&validate.stderr)
    );
    assert!(
        validate_out.contains("deprecated_sandbox_field"),
        "{validate_out}"
    );
    assert!(validate_out.contains("mode = \"Audit\""), "{validate_out}");
    assert!(
        !validate_out.contains("project_security_ratchet"),
        "{validate_out}"
    );

    let status = base_command(home.path())
        .current_dir(cwd.path())
        .arg("status")
        .output()
        .unwrap();
    assert!(status.status.success());
    let status_out = String::from_utf8_lossy(&status.stdout);
    assert!(
        status_out.contains("deprecated_sandbox_field"),
        "{status_out}"
    );
    assert!(status_out.contains("mode = \"Audit\""), "{status_out}");

    // `-c` must NOT surface deprecated_sandbox_field on stdout or stderr.
    let shell_out = base_command(home.path())
        .current_dir(cwd.path())
        .args(["-c", "echo ok"])
        .output()
        .unwrap();
    assert!(shell_out.status.success());
    let shell_combined = format!(
        "{}{}",
        String::from_utf8_lossy(&shell_out.stdout),
        String::from_utf8_lossy(&shell_out.stderr)
    );
    assert!(
        !shell_combined.contains("deprecated_sandbox_field"),
        "-c must not surface deprecated_sandbox_field: {shell_combined}"
    );

    // Config files must remain byte-for-byte unchanged.
    assert_eq!(fs::read_to_string(&global_path).unwrap(), global_body);
    assert_eq!(fs::read_to_string(&project_path).unwrap(), project_body);

    // Misspelled key under [sandbox] still fails load via deny_unknown_fields.
    fs::write(&project_path, "[sandbox]\nenable = true\n").unwrap();
    let bad = base_command(home.path())
        .current_dir(cwd.path())
        .args(["config", "validate"])
        .output()
        .unwrap();
    assert!(
        !bad.status.success(),
        "misspelled sandbox key must fail load"
    );
}

#[test]
fn config_show_omits_deprecated_sandbox_fields() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let output = base_command(home.path())
        .current_dir(cwd.path())
        .args(["config", "show"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "config show failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let shown = String::from_utf8_lossy(&output.stdout);
    let sandbox_section = shown.split("[sandbox]").nth(1).unwrap_or("");
    let sandbox_block = sandbox_section.split("\n[").next().unwrap_or("");
    assert!(!sandbox_block.contains("enabled"), "{sandbox_block}");
    assert!(!sandbox_block.contains("required"), "{sandbox_block}");
}
