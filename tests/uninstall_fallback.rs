#![cfg(unix)]

mod support;

use std::fs;
use support::installer::{installer_path, run_script_at_home};
use tempfile::TempDir;

#[test]
fn fallback_reports_homebrew_and_cargo_without_running_removal_commands() {
    for (channel, expected) in [
        ("brew", "brew uninstall aegis"),
        ("cargo", "cargo uninstall --root '"),
    ] {
        let temp = TempDir::new().unwrap();
        let tools = temp.path().join("tools");
        fs::create_dir_all(&tools).unwrap();
        support::write_executable(&tools.join("aegis"), "#!/bin/sh\nexit 99\n");
        support::write_executable(
            &tools.join("brew"),
            &format!(
                "#!/bin/sh\n[ \"$1\" = list ] && [ \"{channel}\" = brew ] && exit 0\n[ \"$1\" = uninstall ] && printf ran > \"$HOME/manager-ran\"\nexit 1\n"
            ),
        );
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let path = installer_path(&temp, &tools);
        let bindir = temp.path().join("curl-bin");
        let cargo_home = temp.path().join("cargo");
        if channel == "cargo" {
            fs::create_dir_all(cargo_home.join("bin")).unwrap();
            std::os::unix::fs::symlink(tools.join("aegis"), cargo_home.join("bin/aegis")).unwrap();
        }
        let output = run_script_at_home(
            &home,
            "uninstall.sh",
            &[
                ("PATH", &path),
                ("AEGIS_BINDIR", bindir.to_str().unwrap()),
                ("AEGIS_REAL_SHELL", "/bin/bash"),
                ("AEGIS_UNINSTALL_PURGE_DATA", "0"),
                ("CARGO_HOME", cargo_home.to_str().unwrap()),
            ],
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let warning = String::from_utf8_lossy(&output.stderr);
        assert!(warning.contains(expected), "{warning}");
        assert!(!home.join("manager-ran").exists());
        assert!(tools.join("aegis").exists());
    }
}

#[test]
fn fallback_leaves_a_homebrew_cellar_link_alone_and_advises_brew() {
    let temp = TempDir::new().unwrap();
    let tools = temp.path().join("tools");
    fs::create_dir_all(&tools).unwrap();
    let cellar_binary = temp.path().join("Cellar/aegis/1.0/bin/aegis");
    fs::create_dir_all(cellar_binary.parent().unwrap()).unwrap();
    support::write_executable(&cellar_binary, "#!/bin/sh\nexit 99\n");
    let bindir = temp.path().join("bin");
    fs::create_dir_all(&bindir).unwrap();
    // A relative target, as Homebrew writes it.
    std::os::unix::fs::symlink("../Cellar/aegis/1.0/bin/aegis", bindir.join("aegis")).unwrap();
    let home = temp.path().join("home");
    let path = format!("{}:{}", bindir.display(), installer_path(&temp, &tools));
    let output = run_script_at_home(
        &home,
        "uninstall.sh",
        &[
            ("PATH", &path),
            ("AEGIS_BINDIR", bindir.to_str().unwrap()),
            ("AEGIS_REAL_SHELL", "/bin/bash"),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(bindir.join("aegis").is_symlink());
    assert!(cellar_binary.exists());
    assert!(stdout.contains("brew uninstall aegis"), "{stdout}");
    assert!(!stdout.contains("Removed /"), "{stdout}");
    assert!(!stderr.contains("still on PATH"), "{stderr}");
    assert!(!stderr.contains("Homebrew reports"), "{stderr}");
}

#[test]
fn fallback_advises_cargo_for_a_cargo_binary_even_when_homebrew_also_has_aegis() {
    let temp = TempDir::new().unwrap();
    let tools = temp.path().join("tools");
    fs::create_dir_all(&tools).unwrap();
    support::write_executable(
        &tools.join("brew"),
        "#!/bin/sh\n[ \"$1\" = list ] && exit 0\nexit 1\n",
    );
    let cargo_home = temp.path().join("cargo");
    fs::create_dir_all(cargo_home.join("bin")).unwrap();
    support::write_executable(&cargo_home.join("bin/aegis"), "#!/bin/sh\nexit 99\n");
    let home = temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let path = format!(
        "{}:{}",
        cargo_home.join("bin").display(),
        installer_path(&temp, &tools)
    );
    let bindir = temp.path().join("curl-bin");
    let output = run_script_at_home(
        &home,
        "uninstall.sh",
        &[
            ("PATH", &path),
            ("AEGIS_BINDIR", bindir.to_str().unwrap()),
            ("AEGIS_REAL_SHELL", "/bin/bash"),
            ("AEGIS_UNINSTALL_PURGE_DATA", "0"),
            ("CARGO_HOME", cargo_home.to_str().unwrap()),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let warning = String::from_utf8_lossy(&output.stderr);
    assert!(warning.contains("cargo uninstall --root '"), "{warning}");
    assert!(!warning.contains("brew uninstall"), "{warning}");
}
