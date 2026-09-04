//! End-to-end test that the embedded bubblewrap build confines a command when
//! no usable `bwrap` is on `PATH` (ADR-029 §3).
//!
//! Hosts that cannot confine at all — WSL1, or ubuntu-24.04 runners with the
//! AppArmor user-namespace restriction — skip rather than fail: the
//! confinement decision itself is what fails there, not the embedded build,
//! and refusing on such hosts is already pinned by the required-unavailability
//! contract tests. The resolver caches its program choice per process, so this
//! test sets `PATH` to a directory without `bwrap` *before* any sandbox call.

use std::path::PathBuf;

use aegis_sandbox::{SandboxConfig, SandboxExecutor, SandboxProfile, SandboxResult};

/// A `PATH` directory that has `true` and `sh` (so the sandbox probe and the
/// `sh -c` command can exec them) but no `bwrap`, forcing the resolver to fall
/// back to the embedded build.
fn no_bwrap_path_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aegis_no_bwrap_path_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // Symlink `true` so the probe's `-- true` resolves inside the namespace, and
    // `sh` so the sandbox's `sh -c` command resolves.
    let true_path = std::fs::canonicalize("/usr/bin/true")
        .or_else(|_| std::fs::canonicalize("/bin/true"))
        .expect("a `true` binary must exist");
    std::os::unix::fs::symlink(true_path, dir.join("true")).unwrap();
    let sh_path = std::fs::canonicalize("/bin/sh")
        .or_else(|_| std::fs::canonicalize("/usr/bin/sh"))
        .expect("a `sh` binary must exist");
    std::os::unix::fs::symlink(sh_path, dir.join("sh")).unwrap();
    dir
}

#[test]
fn embedded_bwrap_confines_when_no_system_bwrap_on_path() {
    // Point PATH at an empty dir so the resolver must fall back to the embedded
    // build. Restore it afterwards so other tests are unaffected.
    let no_bwrap = no_bwrap_path_dir();
    let old_path = std::env::var_os("PATH");
    // SAFETY: single-threaded test; PATH is restored before the test returns.
    unsafe { std::env::set_var("PATH", &no_bwrap) };

    let config = SandboxConfig {
        required: true,
        ..Default::default()
    };

    // Skip, not panic, when this host cannot confine at all. The probe runs
    // under the no-bwrap PATH, so it also proves the embedded build is the
    // program the resolver cached — the premise the rest of the test relies
    // on. A host that passes this probe but still fails below is a genuine
    // defect, and the panics stay for exactly that case.
    if !aegis_sandbox::sandbox_available_for(&config) {
        eprintln!(
            "skipped: this host cannot create the user namespaces the sandbox \
             needs, so the embedded-build confinement contract is untestable here"
        );
        // SAFETY: single-threaded test; restores the caller's PATH.
        unsafe { std::env::set_var("PATH", old_path.unwrap_or_default()) };
        std::fs::remove_dir_all(&no_bwrap).unwrap();
        return;
    }

    let executor = SandboxExecutor::new(SandboxProfile::from_config(&config));

    // A command that writes to a read-only path must be confined: the write
    // fails inside the namespace, proving the embedded bwrap applied.
    let result = executor.run("echo x > /root/aegis_embedded_bwrap_probe.txt");

    // SAFETY: single-threaded test; restores the caller's PATH.
    unsafe { std::env::set_var("PATH", old_path.unwrap_or_default()) };
    std::fs::remove_dir_all(&no_bwrap).unwrap();

    match result {
        Ok(SandboxResult::Success(code)) => {
            // The write to /root must have failed (non-zero exit), proving the
            // command ran confined rather than unconfined on the host.
            assert_ne!(
                code, 0,
                "write to /root must fail inside the sandbox; got exit {code}"
            );
        }
        Ok(SandboxResult::Unavailable) => {
            panic!("embedded bwrap must be available when required=true")
        }
        Err(e) => panic!("sandbox run failed: {e}"),
    }
}
