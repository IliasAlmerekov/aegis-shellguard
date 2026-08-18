//! Regression tests for issue #211: Landlock must be applied in the innermost
//! process (inside bwrap's mount namespace) so the bwrap + Landlock layers
//! compose, and `PR_SET_NO_NEW_PRIVS` must be set before
//! `landlock_restrict_self`.
//!
//! Both tests are gated on a Landlock-capable host (ABI >= 1) and skip
//! otherwise, so they stay green on kernels without Landlock.

mod support;

use std::process::Command;

use tempfile::TempDir;

use support::aegis_bin;

/// Serialize a single `allow_write` path in the same length-prefixed format the
/// sandbox crate uses (`<len>:<bytes>`), so the inner wrapper can parse it.
fn encode_single_allow_write(path: &std::path::Path) -> String {
    let s = path.to_string_lossy();
    format!("{}:{}", s.len(), s)
}

/// The inner landlock wrapper, invoked directly (no bwrap), must apply Landlock
/// and deny a write outside `allow_write`. This isolates the Landlock layer
/// from bwrap's namespace isolation.
#[test]
fn inner_landlock_wrapper_denies_write_outside_allow_write() {
    if aegis_sandbox::landlock_abi() < 1 {
        eprintln!("skipping: Landlock ABI < 1");
        return;
    }

    let workspace = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();

    let command = format!(
        "echo ok > {}/inside.txt; echo bad > {}/outside.txt",
        workspace.path().display(),
        outside.path().display()
    );

    let output = Command::new(aegis_bin())
        .env(
            "AEGIS_LANDLOCK_ALLOW_WRITE",
            encode_single_allow_write(workspace.path()),
        )
        .args(["--aegis-inner-landlock", "/bin/sh", "-c", &command])
        .output()
        .unwrap();

    assert!(
        workspace.path().join("inside.txt").exists(),
        "write inside allow_write must succeed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !outside.path().join("outside.txt").exists(),
        "write outside allow_write must be denied by Landlock; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// End-to-end: a real command with `sandbox.enabled = true` and a non-empty
/// `allow_write` must execute inside bwrap **and** a write outside
/// `allow_write` must be denied. This is the acceptance criterion for #211.
#[test]
fn sandboxed_command_runs_and_denies_write_outside_allow_write() {
    if aegis_sandbox::landlock_abi() < 1 {
        eprintln!("skipping: Landlock ABI < 1");
        return;
    }

    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();

    // The C3 ratchet keeps the intersection of base and project `allow_write`,
    // so a non-empty `allow_write` must be set in the trusted global layer
    // (the project layer can only tighten to a subset of it).
    support::write_global_config(
        home.path(),
        &format!(
            "[sandbox]\nenabled = true\nallow_write = [\"{}\"]\n",
            workspace.path().display()
        ),
    );

    let command = format!(
        "echo ok > {}/inside.txt; echo bad > {}/outside.txt",
        workspace.path().display(),
        outside.path().display()
    );

    let output = Command::new(aegis_bin())
        .env("AEGIS_REAL_SHELL", "/bin/sh")
        .env("AEGIS_CI", "0")
        .env("HOME", home.path())
        .current_dir(workspace.path())
        .args(["-c", &command])
        .output()
        .unwrap();

    assert!(
        workspace.path().join("inside.txt").exists(),
        "sandboxed command must execute and write inside allow_write; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !outside.path().join("outside.txt").exists(),
        "write outside allow_write must be denied; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
