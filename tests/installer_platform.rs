mod support;

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use support::installer::*;

#[test]
fn install_script_configures_shell_wrapper_block_once() {
    let temp = TempDir::new().unwrap();
    let bindir = temp.path().join("bin");
    let rc_file = temp.path().join(".bashrc");
    let stub_dir = temp.path().join("stub-bin");

    fs::write(&rc_file, "export FOO=bar\n").unwrap();
    let (binary_asset, checksum_asset, binary_digest, path_value) =
        prepare_checksum_ready_release(&temp, &stub_dir);
    let bindir_str = bindir.display().to_string();
    let rc_file_str = rc_file.display().to_string();
    let binary_asset_str = binary_asset.display().to_string();
    let checksum_asset_str = checksum_asset.display().to_string();

    let output = run_script(
        "install.sh",
        &[
            ("AEGIS_BINDIR", &bindir_str),
            ("AEGIS_SHELL_RC", &rc_file_str),
            ("AEGIS_OS", "linux"),
            ("AEGIS_ARCH", "x86_64"),
            ("PATH", &path_value),
            ("SHELL", "/bin/bash"),
            ("AEGIS_REAL_SHELL", "/bin/bash"),
            ("TEST_BINARY_ASSET", &binary_asset_str),
            ("TEST_CHECKSUM_ASSET", &checksum_asset_str),
            ("TEST_BINARY_DIGEST", &binary_digest),
        ],
    );

    assert!(
        output.status.success(),
        "install must succeed: stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let aegis_path = bindir.join("aegis");
    assert!(
        aegis_path.exists(),
        "installer must place binary into bindir"
    );

    let rc_contents = fs::read_to_string(&rc_file).unwrap();
    let expected_block = managed_block(Path::new("/bin/bash"), &aegis_path);
    assert!(
        rc_contents.contains(&expected_block),
        "install must append managed shell-wrapper block; rc contents:\n{rc_contents}"
    );
    assert_eq!(
        rc_contents.matches("# >>> aegis shell setup >>>").count(),
        1
    );

    let second_output = run_script(
        "install.sh",
        &[
            ("AEGIS_BINDIR", &bindir_str),
            ("AEGIS_SHELL_RC", &rc_file_str),
            ("AEGIS_OS", "linux"),
            ("AEGIS_ARCH", "x86_64"),
            ("PATH", &path_value),
            ("SHELL", "/bin/bash"),
            ("AEGIS_REAL_SHELL", "/bin/bash"),
            ("TEST_BINARY_ASSET", &binary_asset_str),
            ("TEST_CHECKSUM_ASSET", &checksum_asset_str),
            ("TEST_BINARY_DIGEST", &binary_digest),
        ],
    );

    assert!(
        second_output.status.success(),
        "second install must also succeed: stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&second_output.stdout),
        String::from_utf8_lossy(&second_output.stderr)
    );

    let rc_contents_after_second_run = fs::read_to_string(&rc_file).unwrap();
    assert_eq!(
        rc_contents_after_second_run, rc_contents,
        "installer must be deterministic and not duplicate the managed block"
    );
}

#[test]
fn install_script_prefers_aegis_real_shell_when_shell_already_points_to_wrapper() {
    let temp = TempDir::new().unwrap();
    let bindir = temp.path().join("bin");
    let rc_file = temp.path().join(".zshrc");
    let stub_dir = temp.path().join("stub-bin");

    fs::write(&rc_file, "").unwrap();
    let (binary_asset, checksum_asset, binary_digest, path_value) =
        prepare_checksum_ready_release(&temp, &stub_dir);
    let bindir_str = bindir.display().to_string();
    let rc_file_str = rc_file.display().to_string();
    let binary_asset_str = binary_asset.display().to_string();
    let checksum_asset_str = checksum_asset.display().to_string();
    let wrapper_shell = bindir.join("aegis");
    let wrapper_shell_str = wrapper_shell.display().to_string();

    let output = run_script(
        "install.sh",
        &[
            ("AEGIS_BINDIR", &bindir_str),
            ("AEGIS_SHELL_RC", &rc_file_str),
            ("AEGIS_OS", "linux"),
            ("AEGIS_ARCH", "x86_64"),
            ("PATH", &path_value),
            ("SHELL", &wrapper_shell_str),
            ("AEGIS_REAL_SHELL", "/bin/zsh"),
            ("TEST_BINARY_ASSET", &binary_asset_str),
            ("TEST_CHECKSUM_ASSET", &checksum_asset_str),
            ("TEST_BINARY_DIGEST", &binary_digest),
        ],
    );

    assert!(
        output.status.success(),
        "install must succeed from an already-wrapped shell when AEGIS_REAL_SHELL is set"
    );

    let rc_contents = fs::read_to_string(&rc_file).unwrap();
    let expected_block = managed_block(Path::new("/bin/zsh"), &wrapper_shell);
    assert!(
        rc_contents.contains(&expected_block),
        "managed block must preserve the real shell to avoid recursion; rc contents:\n{rc_contents}"
    );
}

#[test]
fn install_script_rejects_unsafe_real_shell_value_before_rc_mutation() {
    let temp = TempDir::new().unwrap();
    let bindir = temp.path().join("bin");
    let rc_file = temp.path().join(".bashrc");
    let stub_dir = temp.path().join("stub-bin");

    fs::write(&rc_file, "export FOO=bar\n").unwrap();
    let (binary_asset, checksum_asset, binary_digest, path_value) =
        prepare_checksum_ready_release(&temp, &stub_dir);
    let bindir_str = bindir.display().to_string();
    let rc_file_str = rc_file.display().to_string();
    let binary_asset_str = binary_asset.display().to_string();
    let checksum_asset_str = checksum_asset.display().to_string();

    let output = run_script(
        "install.sh",
        &[
            ("AEGIS_BINDIR", &bindir_str),
            ("AEGIS_SHELL_RC", &rc_file_str),
            ("AEGIS_OS", "linux"),
            ("AEGIS_ARCH", "x86_64"),
            ("PATH", &path_value),
            ("SHELL", "/bin/bash"),
            ("AEGIS_REAL_SHELL", "/bin/bash\nexport EVIL=1"),
            ("TEST_BINARY_ASSET", &binary_asset_str),
            ("TEST_CHECKSUM_ASSET", &checksum_asset_str),
            ("TEST_BINARY_DIGEST", &binary_digest),
        ],
    );

    assert!(
        !output.status.success(),
        "install must reject unsafe real shell values: stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("invalid real shell path"),
        "installer must explain why the shell path was rejected"
    );

    let rc_contents = fs::read_to_string(&rc_file).unwrap();
    assert_eq!(
        rc_contents, "export FOO=bar\n",
        "unsafe real shell values must not mutate the rc file"
    );
    assert!(
        !bindir.join("aegis").exists(),
        "unsafe real shell values must abort before installing the binary"
    );
}

#[test]
fn install_script_rejects_deprecated_setup_controls_before_mutation() {
    for (env_key, env_value) in [
        ("AEGIS_SETUP_MODE", "binary"),
        ("AEGIS_SKIP_SHELL_SETUP", "1"),
    ] {
        let temp = TempDir::new().unwrap();
        let bindir = temp.path().join("bin");
        let rc_file = temp.path().join(".bashrc");
        let stub_dir = temp.path().join("stub-bin");

        fs::write(&rc_file, "export FOO=bar\n").unwrap();
        let (binary_asset, checksum_asset, binary_digest, path_value) =
            prepare_checksum_ready_release(&temp, &stub_dir);
        let bindir_str = bindir.display().to_string();
        let rc_file_str = rc_file.display().to_string();
        let binary_asset_str = binary_asset.display().to_string();
        let checksum_asset_str = checksum_asset.display().to_string();

        let output = run_script(
            "install.sh",
            &[
                ("AEGIS_BINDIR", &bindir_str),
                ("AEGIS_SHELL_RC", &rc_file_str),
                ("AEGIS_OS", "linux"),
                ("AEGIS_ARCH", "x86_64"),
                (env_key, env_value),
                ("PATH", &path_value),
                ("SHELL", "/bin/bash"),
                ("TEST_BINARY_ASSET", &binary_asset_str),
                ("TEST_CHECKSUM_ASSET", &checksum_asset_str),
                ("TEST_BINARY_DIGEST", &binary_digest),
            ],
        );

        assert!(
            !output.status.success(),
            "installer must reject deprecated control {env_key}; stdout=\n{}\nstderr=\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(env_key),
            "error should name the deprecated control {env_key}; stderr=\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !bindir.join("aegis").exists(),
            "deprecated controls must be rejected before touching bindir"
        );
        let rc_contents = fs::read_to_string(&rc_file).unwrap();
        assert_eq!(
            rc_contents, "export FOO=bar\n",
            "deprecated controls must be rejected before mutating rc files"
        );
    }
}

#[test]
fn install_script_rejects_unsupported_shell_before_mutation() {
    let temp = TempDir::new().unwrap();
    let bindir = temp.path().join("bin");
    let rc_file = temp.path().join(".bashrc");
    let stub_dir = temp.path().join("stub-bin");

    fs::write(&rc_file, "export FOO=bar\n").unwrap();
    let (binary_asset, checksum_asset, binary_digest, path_value) =
        prepare_checksum_ready_release(&temp, &stub_dir);
    let bindir_str = bindir.display().to_string();
    let binary_asset_str = binary_asset.display().to_string();
    let checksum_asset_str = checksum_asset.display().to_string();

    let output = run_script(
        "install.sh",
        &[
            ("AEGIS_BINDIR", &bindir_str),
            ("AEGIS_OS", "linux"),
            ("AEGIS_ARCH", "x86_64"),
            ("PATH", &path_value),
            ("SHELL", "/bin/fish"),
            ("TEST_BINARY_ASSET", &binary_asset_str),
            ("TEST_CHECKSUM_ASSET", &checksum_asset_str),
            ("TEST_BINARY_DIGEST", &binary_digest),
        ],
    );

    assert!(
        !output.status.success(),
        "unsupported shells must fail before any installation occurs: stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("automatic shell setup supports bash and zsh"),
        "unsupported shell error should explain the bash/zsh limitation; stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !bindir.join("aegis").exists(),
        "unsupported shell must fail before downloading or installing the binary"
    );
    let rc_contents = fs::read_to_string(&rc_file).unwrap();
    assert_eq!(
        rc_contents, "export FOO=bar\n",
        "unsupported shell must fail before mutating rc files"
    );
}

#[test]
fn install_script_global_setup_writes_shell_setup() {
    let temp = TempDir::new().unwrap();
    let bindir = temp.path().join("bin");
    let rc_file = temp.path().join(".bashrc");
    let stub_dir = temp.path().join("stub-bin");

    fs::write(&rc_file, "").unwrap();
    let (binary_asset, checksum_asset, binary_digest, path_value) =
        prepare_checksum_ready_release(&temp, &stub_dir);
    let bindir_str = bindir.display().to_string();
    let rc_file_str = rc_file.display().to_string();
    let binary_asset_str = binary_asset.display().to_string();
    let checksum_asset_str = checksum_asset.display().to_string();

    let output = run_script(
        "install.sh",
        &[
            ("AEGIS_BINDIR", &bindir_str),
            ("AEGIS_SHELL_RC", &rc_file_str),
            ("AEGIS_OS", "linux"),
            ("AEGIS_ARCH", "x86_64"),
            ("PATH", &path_value),
            ("SHELL", "/bin/bash"),
            ("AEGIS_REAL_SHELL", "/bin/bash"),
            ("TEST_BINARY_ASSET", &binary_asset_str),
            ("TEST_CHECKSUM_ASSET", &checksum_asset_str),
            ("TEST_BINARY_DIGEST", &binary_digest),
        ],
    );

    assert!(
        output.status.success(),
        "global mode must succeed: stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let rc_contents = fs::read_to_string(&rc_file).unwrap();
    let aegis_path = bindir.join("aegis");
    let expected_block = managed_block(Path::new("/bin/bash"), &aegis_path);
    assert!(
        rc_contents.contains(&expected_block),
        "global setup must write managed block; rc contents:\n{rc_contents}"
    );
}

#[test]
fn uninstall_script_removes_managed_block_and_binary() {
    let temp = TempDir::new().unwrap();
    let bindir = temp.path().join("bin");
    let rc_file = temp.path().join(".bashrc");
    let stub_dir = temp.path().join("stub-bin");

    fs::write(&rc_file, "export FOO=bar\n").unwrap();
    let (binary_asset, checksum_asset, binary_digest, path_value) =
        prepare_checksum_ready_release(&temp, &stub_dir);
    let bindir_str = bindir.display().to_string();
    let rc_file_str = rc_file.display().to_string();
    let binary_asset_str = binary_asset.display().to_string();
    let checksum_asset_str = checksum_asset.display().to_string();

    let install_output = run_script(
        "install.sh",
        &[
            ("AEGIS_BINDIR", &bindir_str),
            ("AEGIS_SHELL_RC", &rc_file_str),
            ("AEGIS_OS", "linux"),
            ("AEGIS_ARCH", "x86_64"),
            ("PATH", &path_value),
            ("SHELL", "/bin/bash"),
            ("TEST_BINARY_ASSET", &binary_asset_str),
            ("TEST_CHECKSUM_ASSET", &checksum_asset_str),
            ("TEST_BINARY_DIGEST", &binary_digest),
        ],
    );
    assert!(install_output.status.success());

    let uninstall_output = run_script(
        "uninstall.sh",
        &[
            ("AEGIS_BINDIR", &bindir_str),
            ("AEGIS_SHELL_RC", &rc_file_str),
            ("SHELL", "/bin/bash"),
        ],
    );

    assert!(
        uninstall_output.status.success(),
        "uninstall must succeed: stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&uninstall_output.stdout),
        String::from_utf8_lossy(&uninstall_output.stderr)
    );

    let rc_contents = fs::read_to_string(&rc_file).unwrap();
    assert_eq!(
        rc_contents, "export FOO=bar\n",
        "uninstall must restore the rc file by removing only the managed block"
    );
    assert!(
        !bindir.join("aegis").exists(),
        "uninstall must remove the installed binary"
    );
}

#[test]
fn uninstall_script_does_not_create_missing_rc_file() {
    let temp = TempDir::new().unwrap();
    let bindir = temp.path().join("bin");
    let rc_file = temp.path().join(".bashrc");

    fs::create_dir_all(&bindir).unwrap();
    write_fake_release_binary(&bindir.join("aegis"));
    assert!(
        !rc_file.exists(),
        "test setup must start without an rc file"
    );

    let bindir_str = bindir.display().to_string();
    let rc_file_str = rc_file.display().to_string();

    let uninstall_output = run_script(
        "uninstall.sh",
        &[
            ("AEGIS_BINDIR", &bindir_str),
            ("AEGIS_SHELL_RC", &rc_file_str),
            ("SHELL", "/bin/bash"),
        ],
    );

    assert!(
        uninstall_output.status.success(),
        "uninstall must succeed even when the rc file is absent: stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&uninstall_output.stdout),
        String::from_utf8_lossy(&uninstall_output.stderr)
    );
    assert!(
        !rc_file.exists(),
        "uninstall must not create a missing rc file"
    );
    assert!(
        !bindir.join("aegis").exists(),
        "uninstall must still remove the installed binary when the rc file is absent"
    );
}

#[test]
fn uninstall_script_honors_explicit_rc_override_without_shell_detection() {
    let temp = TempDir::new().unwrap();
    let bindir = temp.path().join("bin");
    let rc_file = temp.path().join(".bashrc");
    let aegis_path = bindir.join("aegis");

    fs::create_dir_all(&bindir).unwrap();
    write_fake_release_binary(&aegis_path);
    fs::write(
        &rc_file,
        format!(
            "export FOO=bar\n{}",
            managed_block(Path::new("/bin/bash"), &aegis_path)
        ),
    )
    .unwrap();

    let bindir_str = bindir.display().to_string();
    let rc_file_str = rc_file.display().to_string();
    let aegis_path_str = aegis_path.display().to_string();

    let uninstall_output = run_script(
        "uninstall.sh",
        &[
            ("AEGIS_BINDIR", &bindir_str),
            ("AEGIS_SHELL_RC", &rc_file_str),
            ("SHELL", &aegis_path_str),
        ],
    );

    assert!(
        uninstall_output.status.success(),
        "uninstall must honor explicit rc override even when SHELL points at aegis: stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&uninstall_output.stdout),
        String::from_utf8_lossy(&uninstall_output.stderr)
    );
    assert_eq!(
        fs::read_to_string(&rc_file).unwrap(),
        "export FOO=bar\n",
        "uninstall must clean the explicit rc file override"
    );
    assert!(
        !aegis_path.exists(),
        "uninstall must still remove the installed binary when using an explicit rc override"
    );
}

#[test]
fn uninstall_script_reports_missing_binary_instead_of_claiming_removal() {
    let temp = TempDir::new().unwrap();
    let bindir = temp.path().join("bin");
    let rc_file = temp.path().join(".bashrc");

    fs::create_dir_all(&bindir).unwrap();
    let bindir_str = bindir.display().to_string();
    let rc_file_str = rc_file.display().to_string();

    let uninstall_output = run_script(
        "uninstall.sh",
        &[
            ("AEGIS_BINDIR", &bindir_str),
            ("AEGIS_SHELL_RC", &rc_file_str),
            ("SHELL", "/bin/bash"),
        ],
    );

    assert!(uninstall_output.status.success());
    let stdout = String::from_utf8_lossy(&uninstall_output.stdout);
    assert!(
        !stdout.contains(&format!("Removed {}", bindir.join("aegis").display())),
        "uninstall must not claim it removed a binary that was never there: stdout=\n{stdout}"
    );
    assert!(
        stdout.contains("No binary found"),
        "uninstall must report that there was no curl-installed binary to remove: stdout=\n{stdout}"
    );
}

#[test]
fn uninstall_script_leaves_data_directory_by_default_and_reports_it() {
    let temp = TempDir::new().unwrap();
    let bindir = temp.path().join("bin");
    let rc_file = temp.path().join(".bashrc");
    let home = temp.path().join("home");

    fs::create_dir_all(&bindir).unwrap();
    let data_dir = home.join(".aegis");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join("audit.jsonl"), "{}\n").unwrap();

    let bindir_str = bindir.display().to_string();
    let rc_file_str = rc_file.display().to_string();

    let uninstall_output = run_script_at_home(
        &home,
        "uninstall.sh",
        &[
            ("AEGIS_BINDIR", &bindir_str),
            ("AEGIS_SHELL_RC", &rc_file_str),
            ("SHELL", "/bin/bash"),
        ],
    );

    assert!(
        uninstall_output.status.success(),
        "uninstall must succeed: stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&uninstall_output.stdout),
        String::from_utf8_lossy(&uninstall_output.stderr)
    );
    assert!(
        data_dir.join("audit.jsonl").exists(),
        "uninstall must not touch the data directory by default"
    );

    let stdout = String::from_utf8_lossy(&uninstall_output.stdout);
    assert!(
        stdout.contains("Left data directory in place"),
        "uninstall must call out that the data directory was intentionally kept: stdout=\n{stdout}"
    );
    assert!(
        stdout.contains("AEGIS_UNINSTALL_PURGE_DATA=1") || stdout.contains("--purge-data"),
        "uninstall must tell the operator how to opt into a full data wipe: stdout=\n{stdout}"
    );
}

#[test]
fn uninstall_script_purges_data_directory_when_requested_via_env() {
    let temp = TempDir::new().unwrap();
    let bindir = temp.path().join("bin");
    let rc_file = temp.path().join(".bashrc");
    let home = temp.path().join("home");

    fs::create_dir_all(&bindir).unwrap();
    let data_dir = home.join(".aegis");
    fs::create_dir_all(data_dir.join("snapshots")).unwrap();
    fs::write(data_dir.join("audit.jsonl"), "{}\n").unwrap();

    let bindir_str = bindir.display().to_string();
    let rc_file_str = rc_file.display().to_string();

    let uninstall_output = run_script_at_home(
        &home,
        "uninstall.sh",
        &[
            ("AEGIS_BINDIR", &bindir_str),
            ("AEGIS_SHELL_RC", &rc_file_str),
            ("SHELL", "/bin/bash"),
            ("AEGIS_UNINSTALL_PURGE_DATA", "1"),
        ],
    );

    assert!(
        uninstall_output.status.success(),
        "uninstall must succeed: stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&uninstall_output.stdout),
        String::from_utf8_lossy(&uninstall_output.stderr)
    );
    assert!(
        !data_dir.exists(),
        "uninstall must delete the data directory when AEGIS_UNINSTALL_PURGE_DATA=1 is set"
    );

    let stdout = String::from_utf8_lossy(&uninstall_output.stdout);
    assert!(
        stdout.contains("Removed data directory"),
        "uninstall must confirm the data directory was purged: stdout=\n{stdout}"
    );
}

#[test]
fn uninstall_script_purges_data_directory_when_requested_via_flag() {
    let temp = TempDir::new().unwrap();
    let bindir = temp.path().join("bin");
    let rc_file = temp.path().join(".bashrc");
    let home = temp.path().join("home");

    fs::create_dir_all(&bindir).unwrap();
    let data_dir = home.join(".aegis");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join("audit.jsonl"), "{}\n").unwrap();

    let bindir_str = bindir.display().to_string();
    let rc_file_str = rc_file.display().to_string();

    let uninstall_output = run_script_at_home_with_args(
        &home,
        "uninstall.sh",
        &["--purge-data"],
        &[
            ("AEGIS_BINDIR", &bindir_str),
            ("AEGIS_SHELL_RC", &rc_file_str),
            ("SHELL", "/bin/bash"),
        ],
    );

    assert!(
        uninstall_output.status.success(),
        "uninstall must succeed: stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&uninstall_output.stdout),
        String::from_utf8_lossy(&uninstall_output.stderr)
    );
    assert!(
        !data_dir.exists(),
        "uninstall must delete the data directory when --purge-data is passed"
    );
}

#[test]
fn uninstall_script_warns_about_a_remaining_binary_still_on_path() {
    let temp = TempDir::new().unwrap();
    let bindir = temp.path().join("bin");
    let rc_file = temp.path().join(".bashrc");
    let home = temp.path().join("home");
    let elsewhere = temp.path().join("elsewhere");

    fs::create_dir_all(&bindir).unwrap();
    fs::create_dir_all(&elsewhere).unwrap();
    write_fake_release_binary(&elsewhere.join("aegis"));

    let bindir_str = bindir.display().to_string();
    let rc_file_str = rc_file.display().to_string();

    let stub_dir = temp.path().join("stub-bin");
    let path_value = installer_path(&temp, &stub_dir);
    let path_with_elsewhere = format!("{}:{}", elsewhere.display(), path_value);

    let uninstall_output = run_script_at_home(
        &home,
        "uninstall.sh",
        &[
            ("AEGIS_BINDIR", &bindir_str),
            ("AEGIS_SHELL_RC", &rc_file_str),
            ("SHELL", "/bin/bash"),
            ("PATH", &path_with_elsewhere),
        ],
    );

    assert!(
        uninstall_output.status.success(),
        "uninstall must succeed even when another aegis binary remains on PATH: stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&uninstall_output.stdout),
        String::from_utf8_lossy(&uninstall_output.stderr)
    );

    let stderr = String::from_utf8_lossy(&uninstall_output.stderr);
    assert!(
        stderr.contains(&elsewhere.join("aegis").display().to_string()),
        "uninstall must warn about the remaining binary's exact path: stderr=\n{stderr}"
    );
}

#[test]
fn uninstall_script_points_at_npm_uninstall_when_npm_owns_the_remaining_binary() {
    let temp = TempDir::new().unwrap();
    let bindir = temp.path().join("bin");
    let rc_file = temp.path().join(".bashrc");
    let home = temp.path().join("home");
    let elsewhere = temp.path().join("elsewhere");

    fs::create_dir_all(&bindir).unwrap();
    fs::create_dir_all(&elsewhere).unwrap();
    write_fake_release_binary(&elsewhere.join("aegis"));

    // A stub `npm` that reports the aegis package as globally installed,
    // standing in for a real npm global install.
    fs::write(
        elsewhere.join("npm"),
        "#!/bin/sh\ncase \"$*\" in\n  'ls -g @iliasalmerekov/aegis') exit 0 ;;\n  *) exit 1 ;;\nesac\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(elsewhere.join("npm")).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(elsewhere.join("npm"), perms).unwrap();
    }

    let bindir_str = bindir.display().to_string();
    let rc_file_str = rc_file.display().to_string();

    let stub_dir = temp.path().join("stub-bin");
    let path_value = installer_path(&temp, &stub_dir);
    let path_with_elsewhere = format!("{}:{}", elsewhere.display(), path_value);

    let uninstall_output = run_script_at_home(
        &home,
        "uninstall.sh",
        &[
            ("AEGIS_BINDIR", &bindir_str),
            ("AEGIS_SHELL_RC", &rc_file_str),
            ("SHELL", "/bin/bash"),
            ("PATH", &path_with_elsewhere),
        ],
    );

    assert!(uninstall_output.status.success());
    let stderr = String::from_utf8_lossy(&uninstall_output.stderr);
    assert!(
        stderr.contains("npm uninstall -g"),
        "uninstall must point the operator at the exact npm uninstall command: stderr=\n{stderr}"
    );
    assert!(
        stderr.contains("@iliasalmerekov/aegis"),
        "uninstall must name the npm package: stderr=\n{stderr}"
    );
}
