#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail loudly"
)]
#[path = "../../../tests/support/isolated_test.rs"]
mod isolated_test;

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use aegis_sandbox::SandboxConfig;

fn no_bwrap_path_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let true_path = std::fs::canonicalize("/usr/bin/true")
        .or_else(|_| std::fs::canonicalize("/bin/true"))
        .expect("a `true` binary must exist");
    std::os::unix::fs::symlink(true_path, dir.path().join("true")).unwrap();
    let sh_path = std::fs::canonicalize("/bin/sh")
        .or_else(|_| std::fs::canonicalize("/usr/bin/sh"))
        .expect("a `sh` binary must exist");
    std::os::unix::fs::symlink(sh_path, dir.path().join("sh")).unwrap();
    dir
}

#[test]
fn embedded_bwrap_confines_when_no_system_bwrap_on_path() {
    const CHILD: &str = "AEGIS_EMBEDDED_BWRAP_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let no_bwrap = no_bwrap_path_dir();
        let mut command = isolated_test::command();
        command
            .env(CHILD, "1")
            .env("PATH", no_bwrap.path())
            .env("AEGIS_NO_BWRAP_DIR", no_bwrap.path());
        isolated_test::run(&mut command);
    } else {
        let _completion = isolated_test::Completion;
        let no_bwrap = PathBuf::from(std::env::var_os("AEGIS_NO_BWRAP_DIR").unwrap());

        let config = SandboxConfig::default();

        if !aegis_sandbox::sandbox_available_for(&config) {
            assert!(
                std::env::var_os("AEGIS_CI_REQUIRE_SANDBOX").as_deref()
                    != Some(std::ffi::OsStr::new("1")),
                "AEGIS_CI_REQUIRE_SANDBOX=1 is set, but embedded bwrap cannot confine on this host"
            );
            eprintln!(
                "skipped: this host cannot create the user namespaces the sandbox \
             needs, so the embedded-build confinement contract is untestable here"
            );
        } else {
            let probe = no_bwrap.join("aegis_embedded_bwrap_probe.txt");
            let args = [
                OsString::from("-c"),
                OsString::from(format!("echo x > '{}'", probe.display())),
            ];
            let prepared = aegis_sandbox::prepare_for_spawn(&config, OsStr::new("sh"), &args);
            let status = prepared.map(|mut p| p.command.status());
            let probe_written = probe.exists();

            let status = status
                .unwrap_or_else(|e| panic!("prepare_for_spawn failed after the probe passed: {e}"))
                .expect("the confined command must spawn");
            assert!(
                !status.success() && !probe_written,
                "write to a host-writable path must fail inside the sandbox; got {status}"
            );
        }
    }
}
