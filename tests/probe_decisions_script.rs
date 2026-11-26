#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail loudly"
)]
mod support;

use std::fs;
use std::process::Command;

use support::repo_path;
use tempfile::TempDir;

const AEGIS: &str = env!("CARGO_BIN_EXE_aegis");

fn probe(cases: &str, home: &TempDir, tmp: &TempDir) -> std::process::Output {
    let cases_file = tmp.path().join("cases.txt");
    fs::write(&cases_file, cases).unwrap();
    Command::new("bash")
        .arg(repo_path("scripts/probe-decisions.sh"))
        .arg(AEGIS)
        .arg(&cases_file)
        .env("HOME", home.path())
        .env("TMPDIR", tmp.path())
        .output()
        .expect("bash must run")
}

#[test]
fn rows_carry_decision_risk_snapshot_and_analysis_and_leave_nothing_behind() {
    let home = TempDir::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let out = probe(
        "# comment\n\nls -la\n$FOO bar\nrm -rf artifact\n",
        &home,
        &tmp,
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stdout}{stderr}");

    let rows: Vec<Vec<&str>> = stdout.lines().map(|l| l.split('\t').collect()).collect();
    assert_eq!(rows.len(), 3, "{stdout}");
    assert_eq!(rows[0][..4], ["ls -la", "auto_approve", "safe", "false"]);
    assert_eq!(rows[1][..3], ["$FOO bar", "prompt", "safe"]);
    assert_eq!(rows[1][4..], ["degraded", "dynamic_source"]);
    assert_eq!(
        rows[2][..4],
        ["rm -rf artifact", "prompt", "danger", "true"]
    );

    let left = |d: &TempDir| fs::read_dir(d.path()).unwrap().count();
    assert_eq!(left(&home), 0, "script wrote into the real HOME");
    assert_eq!(
        left(&tmp),
        1,
        "script left files in TMPDIR besides cases.txt"
    );
}

#[test]
fn help_documents_arguments_columns_and_isolation() {
    let home = TempDir::new().unwrap();
    let out = Command::new("bash")
        .arg(repo_path("scripts/probe-decisions.sh"))
        .arg("--help")
        .env("HOME", home.path())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}");
    for word in [
        "cases",
        "decision",
        "risk",
        "snapshot",
        "degradation",
        "HOME",
        "GIT_DIR",
    ] {
        assert!(text.contains(word), "--help lacks {word}: {text}");
    }
}

#[test]
fn caller_git_config_and_hooks_do_not_reach_the_probe_workspace() {
    use std::os::unix::fs::PermissionsExt;

    let home = TempDir::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let hooks = home.path().join("hooks");
    fs::create_dir(&hooks).unwrap();
    let hook = hooks.join("pre-commit");
    fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(
        home.path().join(".gitconfig"),
        format!("[core]\n\thooksPath = {}\n", hooks.display()),
    )
    .unwrap();

    let out = probe("ls -la\n", &home, &tmp);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stdout}{stderr}");
    assert!(stdout.starts_with("ls -la\tauto_approve"), "{stdout}");
}
