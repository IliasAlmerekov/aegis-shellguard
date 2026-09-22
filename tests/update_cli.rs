//! CLI-level coverage for `aegis update` and the shell wrapper's notice gate
//! (ADR-038). Unit coverage for the pure eligibility/version/lock logic lives
//! in `src/update/`; this file exercises the compiled binary end to end,
//! including the interactive-TTY-only notice gate via a `script`(1) pty,
//! mirroring the pattern `tests/installer_tty.rs` already uses.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

fn aegis_bin() -> &'static str {
    env!("CARGO_BIN_EXE_aegis")
}

/// A `curl` stand-in on `PATH` so these tests never touch the real network.
/// Its response is controlled entirely by `AEGIS_TEST_STUB_MODE` /
/// `AEGIS_TEST_STUB_VERSION` env vars, set per invocation.
fn write_curl_stub(dir: &Path) {
    let script = "#!/bin/sh\n\
case \"$AEGIS_TEST_STUB_MODE\" in\n\
  fail) exit 22 ;;\n\
  malformed) echo 'not json'; exit 0 ;;\n\
  missing-field) echo '{\"name\":\"x\"}'; exit 0 ;;\n\
  *) echo \"{\\\"version\\\":\\\"${AEGIS_TEST_STUB_VERSION:-9.9.9}\\\"}\"; exit 0 ;;\n\
esac\n";
    let path = dir.join("curl");
    fs::write(&path, script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn stubbed_path(stub_dir: &Path) -> String {
    format!(
        "{}:{}",
        stub_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

/// Run the compiled `aegis` binary against a fake `HOME` and a stubbed `curl`
/// on `PATH`, forcing the non-CI path regardless of the ambient CI env (the
/// outer test runner may itself be `CI=true`) — the convention already used
/// by `tests/cli_integration.rs` and friends.
fn run(
    home: &Path,
    path: &str,
    extra_envs: &[(&str, &str)],
    args: &[&str],
) -> std::process::Output {
    let mut command = Command::new(aegis_bin());
    command
        .env("HOME", home)
        .env("PATH", path)
        .env("AEGIS_CI", "0")
        .args(args);
    for (key, value) in extra_envs {
        command.env(key, value);
    }
    command.output().unwrap()
}

#[test]
fn status_defaults_to_no_consent_and_unknown_cache() {
    let home = TempDir::new().unwrap();
    let stub = TempDir::new().unwrap();
    write_curl_stub(stub.path());

    let output = run(
        home.path(),
        &stubbed_path(stub.path()),
        &[],
        &["update", "status"],
    );
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(output.status.success());
    assert!(stdout.contains("consent: false"));
    assert!(stdout.contains("channel: none"));
    assert!(stdout.contains("latest known version: unknown"));
    assert!(stdout.contains("last successful check: never"));
}

#[test]
fn enable_sets_consent_and_npm_channel() {
    let home = TempDir::new().unwrap();
    let stub = TempDir::new().unwrap();
    write_curl_stub(stub.path());
    let path = stubbed_path(stub.path());

    let enable = run(
        home.path(),
        &path,
        &[],
        &["update", "enable", "--channel", "npm"],
    );
    assert!(enable.status.success());

    let status = run(home.path(), &path, &[], &["update", "status"]);
    let stdout = String::from_utf8(status.stdout).unwrap();
    assert!(stdout.contains("consent: true"));
    assert!(stdout.contains("channel: npm"));
}

#[test]
fn disable_clears_consent_but_keeps_the_cached_version() {
    let home = TempDir::new().unwrap();
    let stub = TempDir::new().unwrap();
    write_curl_stub(stub.path());
    let path = stubbed_path(stub.path());

    run(
        home.path(),
        &path,
        &[],
        &["update", "enable", "--channel", "npm"],
    );
    let check = run(
        home.path(),
        &path,
        &[("AEGIS_TEST_STUB_VERSION", "9.9.9")],
        &["update", "check"],
    );
    assert!(check.status.success());

    let disable = run(home.path(), &path, &[], &["update", "disable"]);
    assert!(disable.status.success());

    let status = run(home.path(), &path, &[], &["update", "status"]);
    let stdout = String::from_utf8(status.stdout).unwrap();
    assert!(stdout.contains("consent: false"));
    assert!(stdout.contains("channel: none"));
    assert!(stdout.contains("latest known version: 9.9.9"));
}

#[test]
fn check_persists_the_registry_version_on_success() {
    let home = TempDir::new().unwrap();
    let stub = TempDir::new().unwrap();
    write_curl_stub(stub.path());
    let path = stubbed_path(stub.path());

    let check = run(
        home.path(),
        &path,
        &[("AEGIS_TEST_STUB_VERSION", "1.2.3")],
        &["update", "check"],
    );

    assert!(check.status.success());
    assert!(String::from_utf8(check.stdout).unwrap().contains("1.2.3"));

    let status = run(home.path(), &path, &[], &["update", "status"]);
    let stdout = String::from_utf8(status.stdout).unwrap();
    assert!(stdout.contains("latest known version: 1.2.3"));
    assert!(!stdout.contains("last successful check: never"));
}

#[test]
fn check_with_curl_unavailable_fails_closed_without_crashing() {
    let home = TempDir::new().unwrap();
    let empty_bin = TempDir::new().unwrap();
    let empty_path = empty_bin.path().display().to_string();

    let check = run(home.path(), &empty_path, &[], &["update", "check"]);

    assert!(!check.status.success());
    assert!(
        String::from_utf8(check.stderr)
            .unwrap()
            .contains("update check failed")
    );

    let status = run(home.path(), &empty_path, &[], &["update", "status"]);
    assert!(
        String::from_utf8(status.stdout)
            .unwrap()
            .contains("latest known version: unknown")
    );
}

#[test]
fn check_with_a_malformed_response_fails_closed_and_keeps_the_previous_cache() {
    let home = TempDir::new().unwrap();
    let stub = TempDir::new().unwrap();
    write_curl_stub(stub.path());
    let path = stubbed_path(stub.path());

    run(
        home.path(),
        &path,
        &[("AEGIS_TEST_STUB_VERSION", "5.0.0")],
        &["update", "check"],
    );

    let malformed = run(
        home.path(),
        &path,
        &[("AEGIS_TEST_STUB_MODE", "malformed")],
        &["update", "check"],
    );
    assert!(!malformed.status.success());

    let status = run(home.path(), &path, &[], &["update", "status"]);
    assert!(
        String::from_utf8(status.stdout)
            .unwrap()
            .contains("latest known version: 5.0.0")
    );
}

/// The shell wrapper's non-interactive text path (default when stdout is
/// piped, as it always is under `Command::output()`) must never print the
/// notice, even with consent on and a strictly newer cached version.
#[test]
fn notice_never_prints_on_a_non_interactive_wrapper_invocation() {
    let home = TempDir::new().unwrap();
    let stub = TempDir::new().unwrap();
    write_curl_stub(stub.path());
    let path = stubbed_path(stub.path());

    run(
        home.path(),
        &path,
        &[],
        &["update", "enable", "--channel", "npm"],
    );
    run(
        home.path(),
        &path,
        &[("AEGIS_TEST_STUB_VERSION", "99.0.0")],
        &["update", "check"],
    );

    let wrapped = run(home.path(), &path, &[], &["-c", "true"]);

    assert!(
        !String::from_utf8_lossy(&wrapped.stderr).contains("is available"),
        "the update notice must stay silent off an interactive TTY"
    );
}

fn script_bin() -> Option<std::path::PathBuf> {
    std::env::var_os("PATH")?
        .to_str()?
        .split(':')
        .map(Path::new)
        .map(|dir| dir.join("script"))
        .find(|candidate| candidate.is_file())
}

/// On a genuine interactive TTY (via `script`(1), as `tests/installer_tty.rs`
/// already does for the installers), a strictly newer cached version prints
/// the notice; an older cached version stays silent.
#[test]
fn notice_prints_on_a_real_tty_only_for_a_strictly_newer_version() {
    let Some(script) = script_bin() else {
        eprintln!("skipping: `script`(1) not found on PATH");
        return;
    };
    let home = TempDir::new().unwrap();
    let stub = TempDir::new().unwrap();
    write_curl_stub(stub.path());
    let path = stubbed_path(stub.path());

    run(
        home.path(),
        &path,
        &[],
        &["update", "enable", "--channel", "npm"],
    );
    run(
        home.path(),
        &path,
        &[("AEGIS_TEST_STUB_VERSION", "0.0.1")],
        &["update", "check"],
    );

    let shell_cmd = format!("{} -c true", aegis_bin());
    let run_under_pty = |script: &Path, home: &Path, path: &str| -> std::process::Output {
        Command::new(script)
            .args(["-qec", &shell_cmd, "/dev/null"])
            .env("HOME", home)
            .env("PATH", path)
            .env("AEGIS_CI", "0")
            .output()
            .unwrap()
    };

    let quiet = run_under_pty(&script, home.path(), &path);
    assert!(
        !String::from_utf8_lossy(&quiet.stdout).contains("is available"),
        "an older cached version must never trigger a notice"
    );

    run(
        home.path(),
        &path,
        &[("AEGIS_TEST_STUB_VERSION", "99.0.0")],
        &["update", "check"],
    );

    let loud = run_under_pty(&script, home.path(), &path);
    assert!(
        String::from_utf8_lossy(&loud.stdout).contains("99.0.0 is available"),
        "a strictly newer cached version must print the notice on a real TTY"
    );
}
