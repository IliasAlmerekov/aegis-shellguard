use std::fs;
use std::process::Command;

use tempfile::TempDir;

fn aegis_bin() -> &'static str {
    env!("CARGO_BIN_EXE_aegis")
}

fn run_aegis_in(home: &TempDir, cwd: &TempDir, args: &[&str]) -> std::process::Output {
    Command::new(aegis_bin())
        .env("HOME", home.path())
        .current_dir(cwd.path())
        .args(args)
        .output()
        .unwrap()
}

fn write_invalid_config(cwd: &TempDir) {
    fs::write(
        cwd.path().join(".aegis.toml"),
        "mode = <<<THIS IS NOT VALID TOML\n",
    )
    .unwrap();
}

#[test]
fn off_creates_disabled_flag_and_status_reports_disabled() {
    let home = TempDir::new().unwrap();
    let output = Command::new(aegis_bin())
        .env("HOME", home.path())
        .args(["off"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(home.path().join(".aegis").join("disabled").exists());

    let status = Command::new(aegis_bin())
        .env("HOME", home.path())
        .args(["status"])
        .output()
        .unwrap();

    let stdout = String::from_utf8(status.stdout).unwrap();
    assert!(stdout.contains("toggle: disabled"));
}

#[test]
fn status_reports_disabled_but_ci_override_active() {
    let home = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join(".aegis")).unwrap();
    fs::write(
        home.path().join(".aegis").join("disabled"),
        "timestamp=x\npid=1\n",
    )
    .unwrap();

    let status = Command::new(aegis_bin())
        .env("HOME", home.path())
        .env("CI", "true")
        .args(["status"])
        .output()
        .unwrap();

    let stdout = String::from_utf8(status.stdout).unwrap();
    assert_eq!(status.status.code(), Some(0));
    assert!(stdout.contains("toggle: disabled"));
    assert!(stdout.contains("effective mode: enforcing (CI override)"));
}

#[test]
fn status_returns_zero_when_enabled_or_disabled() {
    let home = TempDir::new().unwrap();

    let enabled = Command::new(aegis_bin())
        .env("HOME", home.path())
        .args(["status"])
        .output()
        .unwrap();
    assert_eq!(enabled.status.code(), Some(0));

    fs::create_dir_all(home.path().join(".aegis")).unwrap();
    fs::write(
        home.path().join(".aegis").join("disabled"),
        "timestamp=x\npid=1\n",
    )
    .unwrap();

    let disabled = Command::new(aegis_bin())
        .env("HOME", home.path())
        .args(["status"])
        .output()
        .unwrap();
    assert_eq!(disabled.status.code(), Some(0));
}

#[test]
fn disabled_shell_wrapper_passthrough_stays_quiet() {
    let home = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join(".aegis")).unwrap();
    fs::write(
        home.path().join(".aegis").join("disabled"),
        "timestamp=x\npid=1\n",
    )
    .unwrap();

    let output = Command::new(aegis_bin())
        .env("HOME", home.path())
        .env("AEGIS_REAL_SHELL", "/bin/sh")
        .args(["--command", "printf test"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "test");
    assert!(output.stderr.is_empty(), "disabled mode should stay quiet");
}

#[test]
fn status_does_not_claim_ci_override_when_toggle_is_enabled() {
    let home = TempDir::new().unwrap();

    let status = Command::new(aegis_bin())
        .env("HOME", home.path())
        .env("CI", "true")
        .args(["status"])
        .output()
        .unwrap();

    let stdout = String::from_utf8(status.stdout).unwrap();
    assert_eq!(status.status.code(), Some(0));
    assert!(stdout.contains("toggle: enabled"));
    assert!(stdout.contains("effective mode: enforcing"));
    assert!(!stdout.contains("CI override"));
}

#[test]
fn off_still_disables_when_config_is_invalid() {
    // Toggle succeeds (disabled flag is written) but audit append fails when
    // the config is unparseable. Exit code must be non-zero — audit is a
    // security artifact and write failures are hard errors.
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    write_invalid_config(&cwd);

    let output = run_aegis_in(&home, &cwd, &["off"]);

    assert_eq!(
        output.status.code(),
        Some(4),
        "audit write failure must exit non-zero"
    );
    assert!(home.path().join(".aegis").join("disabled").exists());

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("audit entry could not be recorded"),
        "audit failure must be reported to stderr; stderr:\n{stderr}"
    );
}

#[test]
fn on_still_enables_when_config_is_invalid() {
    // Toggle succeeds (disabled flag is removed) but audit append fails when
    // the config is unparseable. Exit code must be non-zero — audit is a
    // security artifact and write failures are hard errors.
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    write_invalid_config(&cwd);
    fs::create_dir_all(home.path().join(".aegis")).unwrap();
    fs::write(
        home.path().join(".aegis").join("disabled"),
        "timestamp=x\npid=1\n",
    )
    .unwrap();

    let output = run_aegis_in(&home, &cwd, &["on"]);

    assert_eq!(
        output.status.code(),
        Some(4),
        "audit write failure must exit non-zero"
    );
    assert!(!home.path().join(".aegis").join("disabled").exists());

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("audit entry could not be recorded"),
        "audit failure must be reported to stderr; stderr:\n{stderr}"
    );
}

#[test]
fn falsy_aegis_ci_keeps_disabled_toggle_in_passthrough_even_with_truthy_ci_env() {
    let home = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join(".aegis")).unwrap();
    fs::write(
        home.path().join(".aegis").join("disabled"),
        "timestamp=x\npid=1\n",
    )
    .unwrap();

    for value in ["0", "false", "no"] {
        let status = Command::new(aegis_bin())
            .env("HOME", home.path())
            .env("AEGIS_CI", value)
            .env("CI", "true")
            .args(["status"])
            .output()
            .unwrap();

        let stdout = String::from_utf8(status.stdout).unwrap();
        assert_eq!(status.status.code(), Some(0));
        assert!(stdout.contains("toggle: disabled"));
        assert!(
            stdout.contains("effective mode: disabled passthrough"),
            "AEGIS_CI={value} should override truthy CI env"
        );
        assert!(!stdout.contains("CI override"));
    }
}

#[test]
fn truthy_aegis_ci_forces_enforcing_ci_override() {
    let home = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join(".aegis")).unwrap();
    fs::write(
        home.path().join(".aegis").join("disabled"),
        "timestamp=x\npid=1\n",
    )
    .unwrap();

    for value in ["1", "true", "yes"] {
        let status = Command::new(aegis_bin())
            .env("HOME", home.path())
            .env("AEGIS_CI", value)
            .env("CI", "false")
            .args(["status"])
            .output()
            .unwrap();

        let stdout = String::from_utf8(status.stdout).unwrap();
        assert_eq!(status.status.code(), Some(0));
        assert!(stdout.contains("toggle: disabled"));
        assert!(
            stdout.contains("effective mode: enforcing (CI override)"),
            "AEGIS_CI={value} should force CI override"
        );
    }
}

/// The Toggle is an operator escape hatch, so the acceptance criterion for M3a
/// requires the transition itself to be auditable — the session-start notice is
/// informational and deliberately records nothing. The failure path is covered
/// above; this pins the successful half, which nothing else asserted.
#[test]
fn a_successful_toggle_appends_an_audit_entry_for_each_transition() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();

    assert!(run_aegis_in(&home, &cwd, &["off"]).status.success());
    assert!(run_aegis_in(&home, &cwd, &["on"]).status.success());

    let audit_log = home.path().join(".aegis").join("audit.jsonl");
    let contents = fs::read_to_string(&audit_log).unwrap_or_else(|err| {
        panic!(
            "a successful toggle must append to {}: {err}",
            audit_log.display()
        )
    });
    let commands: Vec<String> = contents
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .expect("each audit line must be JSON")["command"]
                .as_str()
                .expect("each audit entry must carry a command")
                .to_owned()
        })
        .collect();

    assert!(
        commands.iter().any(|command| command == "aegis off"),
        "disabling must be audited; audited commands: {commands:?}"
    );
    assert!(
        commands.iter().any(|command| command == "aegis on"),
        "re-enabling must be audited; audited commands: {commands:?}"
    );
}
