#![cfg(unix)]

mod support;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::json;
use tempfile::TempDir;

fn uninstall(home: &Path, args: &[&str]) -> Output {
    Command::new(support::aegis_bin())
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .args(["uninstall"])
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn uninstall_removes_managed_integrations_but_keeps_user_hooks_data_and_binary() {
    let home = TempDir::new().unwrap();
    let hooks = home.path().join(".claude/hooks");
    fs::create_dir_all(&hooks).unwrap();
    fs::write(hooks.join("aegis-pre-tool-use.sh"), "managed").unwrap();
    fs::write(hooks.join("user.sh"), "user").unwrap();
    let settings = home.path().join(".claude/settings.json");
    fs::write(
        &settings,
        json!({"theme": "dark", "hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [
            {"type": "command", "command": "aegis hook"},
            {"type": "command", "command": "echo keep"}
        ]}]}})
        .to_string(),
    )
    .unwrap();
    let rc = home.path().join(".zshrc");
    fs::write(&rc, "alias ll='ls -la'\n# >>> aegis shell setup >>>\nexport SHELL='/tmp/aegis'\n# <<< aegis shell setup <<<\n").unwrap();
    fs::create_dir_all(home.path().join(".aegis/snapshots")).unwrap();
    let audit = home.path().join(".aegis/audit.jsonl");
    fs::write(&audit, "keep audit\n").unwrap();

    let output = uninstall(home.path(), &["--channel", "homebrew"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let message = String::from_utf8(output.stdout).unwrap();
    assert!(message.contains("brew uninstall aegis"), "{message}");
    assert!(message.contains("kept"), "{message}");
    assert!(!hooks.join("aegis-pre-tool-use.sh").exists());
    assert_eq!(fs::read_to_string(hooks.join("user.sh")).unwrap(), "user");
    assert_eq!(fs::read_to_string(&rc).unwrap(), "alias ll='ls -la'\n");
    assert_eq!(fs::read_to_string(&audit).unwrap(), "keep audit\n");
    let remaining: serde_json::Value =
        serde_json::from_slice(&fs::read(settings).unwrap()).unwrap();
    assert_eq!(remaining["theme"], "dark");
    assert_eq!(
        remaining["hooks"]["PreToolUse"][0]["hooks"],
        json!([
            {"type": "command", "command": "echo keep"}
        ])
    );
    assert!(support::aegis_bin().is_file());
}

#[test]
fn uninstall_requires_explicit_data_purge_and_keeps_user_configuration() {
    let home = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join(".aegis/snapshots")).unwrap();
    fs::write(home.path().join(".aegis/snapshots/backup"), "backup").unwrap();
    fs::create_dir_all(home.path().join(".config/aegis")).unwrap();
    fs::write(home.path().join(".config/aegis/config.toml"), "keep").unwrap();
    let output = uninstall(home.path(), &["--purge-data", "--channel", "npm"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!home.path().join(".aegis").exists());
    assert_eq!(
        fs::read_to_string(home.path().join(".config/aegis/config.toml")).unwrap(),
        "keep"
    );
}

#[test]
fn uninstall_refuses_symlinked_data_before_changing_integrations() {
    let home = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("audit.jsonl"), "private").unwrap();
    std::os::unix::fs::symlink(outside.path(), home.path().join(".aegis")).unwrap();
    fs::create_dir_all(home.path().join(".codex/hooks")).unwrap();
    let hook = home.path().join(".codex/hooks/aegis-pre-tool-use.sh");
    fs::write(&hook, "managed").unwrap();
    let output = uninstall(home.path(), &["--purge-data"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("symlink"));
    assert!(hook.exists());
    assert_eq!(
        fs::read_to_string(outside.path().join("audit.jsonl")).unwrap(),
        "private"
    );
}

#[test]
fn uninstall_rejects_invalid_settings_without_deleting_hook_payloads() {
    let home = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join(".codex/hooks")).unwrap();
    let hook = home.path().join(".codex/hooks/aegis-session-start.sh");
    fs::write(&hook, "managed").unwrap();
    fs::write(home.path().join(".codex/hooks.json"), "not JSON").unwrap();
    let output = uninstall(home.path(), &[]);
    assert!(!output.status.success());
    assert!(hook.exists());
}

#[test]
fn uninstall_reports_exact_commands_without_running_package_managers() {
    for (channel, expected) in [
        ("npm", "npm uninstall -g @iliasalmerekov/aegis"),
        ("homebrew", "brew uninstall aegis"),
        ("cargo", "cargo uninstall aegis"),
        ("curl", "rm -- '"),
    ] {
        let home = TempDir::new().unwrap();
        let tools = home.path().join("tools");
        fs::create_dir_all(&tools).unwrap();
        for name in ["npm", "brew", "cargo"] {
            support::write_executable(
                &tools.join(name),
                "#!/bin/sh\nprintf ran > \"$HOME/manager-ran\"\nexit 1\n",
            );
        }
        let output = Command::new(support::aegis_bin())
            .env_clear()
            .env("HOME", home.path())
            .env("PATH", &tools)
            .args(["uninstall", "--channel", channel])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains(expected));
        assert!(!home.path().join("manager-ran").exists());
    }
}

#[test]
fn uninstall_does_not_guess_global_npm_for_a_project_local_binary() {
    let home = TempDir::new().unwrap();
    let binary = home
        .path()
        .join("project/node_modules/@iliasalmerekov/aegis/bin/aegis");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    assert!(
        Command::new("/bin/cp")
            .arg(support::aegis_bin())
            .arg(&binary)
            .status()
            .unwrap()
            .success()
    );
    let output = Command::new(binary)
        .env_clear()
        .env("HOME", home.path())
        .args(["uninstall"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let message = String::from_utf8_lossy(&output.stdout);
    assert!(message.contains("Removal channel unknown"), "{message}");
    assert!(!message.contains("channel inferred"), "{message}");
    assert!(!message.contains("npm uninstall -g"), "{message}");
}

#[test]
fn uninstall_preserves_settings_permissions_and_unrelated_empty_hook_entries() {
    use std::os::unix::fs::PermissionsExt;
    let home = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join(".codex")).unwrap();
    let path = home.path().join(".codex/hooks.json");
    fs::write(
        &path,
        json!({"hooks": {"PreToolUse": [
            {"matcher": "User", "hooks": []},
            {"matcher": "Bash", "hooks": [{"type": "command", "command": "aegis hook"}]}
        ]}})
        .to_string(),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let output = uninstall(home.path(), &[]);
    assert!(output.status.success());
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let value: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(
        value["hooks"]["PreToolUse"],
        json!([{"matcher": "User", "hooks": []}])
    );
}

#[test]
fn uninstall_is_idempotent_and_does_not_create_missing_agent_or_shell_files() {
    let home = TempDir::new().unwrap();
    for _ in 0..2 {
        assert!(uninstall(home.path(), &[]).status.success());
        for path in [".bashrc", ".zshrc", ".claude", ".codex", ".aegis"] {
            assert!(!home.path().join(path).exists());
        }
    }
}

#[test]
fn uninstall_infers_known_binary_layouts_without_deleting_the_binary() {
    for (layout, expected) in [
        (
            "prefix/lib/node_modules/@iliasalmerekov/aegis/bin/aegis",
            "npm uninstall -g @iliasalmerekov/aegis",
        ),
        ("prefix/Cellar/aegis/1.0/bin/aegis", "brew uninstall aegis"),
        (".cargo/bin/aegis", "cargo uninstall aegis"),
        ("curl-bin/aegis", "rm -- '"),
        ("unknown/aegis", "Removal channel unknown"),
    ] {
        let home = TempDir::new().unwrap();
        let binary = home.path().join(layout);
        fs::create_dir_all(binary.parent().unwrap()).unwrap();
        assert!(
            Command::new("/bin/cp")
                .arg(support::aegis_bin())
                .arg(&binary)
                .status()
                .unwrap()
                .success()
        );
        let output = Command::new(&binary)
            .env_clear()
            .env("HOME", home.path())
            .env("AEGIS_BINDIR", home.path().join("curl-bin"))
            .args(["uninstall"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let message = String::from_utf8_lossy(&output.stdout);
        assert!(message.contains(expected), "{message}");
        assert!(binary.exists());
    }
}

#[test]
fn uninstall_preserves_custom_rc_permissions_and_prunes_codex_session_start() {
    use std::os::unix::fs::PermissionsExt;
    let home = TempDir::new().unwrap();
    let rc = home.path().join("custom rc");
    fs::write(
        &rc,
        "keep\n# >>> aegis shell setup >>>\nmanaged\n# <<< aegis shell setup <<<\n",
    )
    .unwrap();
    fs::set_permissions(&rc, fs::Permissions::from_mode(0o640)).unwrap();
    fs::create_dir_all(home.path().join(".codex")).unwrap();
    let settings = home.path().join(".codex/hooks.json");
    fs::write(
        &settings,
        json!({"hooks":{"SessionStart":[{"hooks":[
            {"type":"command","command":home.path().join(".codex/hooks/aegis-session-start.sh")},
            {"type":"command","command":"echo keep"}
        ]}]}})
        .to_string(),
    )
    .unwrap();
    assert!(
        uninstall(home.path(), &["--rc-file", rc.to_str().unwrap()])
            .status
            .success()
    );
    assert_eq!(fs::read_to_string(&rc).unwrap(), "keep\n");
    assert_eq!(
        fs::metadata(&rc).unwrap().permissions().mode() & 0o777,
        0o640
    );
    let remaining: serde_json::Value =
        serde_json::from_slice(&fs::read(settings).unwrap()).unwrap();
    assert_eq!(
        remaining["hooks"]["SessionStart"][0]["hooks"],
        json!([{"type":"command","command":"echo keep"}])
    );
}

#[test]
fn uninstall_preflights_malformed_rc_and_symlinked_settings_or_rc() {
    for fixture in ["malformed", "settings-link", "rc-link"] {
        let home = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        fs::write(outside.path().join("target"), "keep").unwrap();
        fs::create_dir_all(home.path().join(".claude/hooks")).unwrap();
        let hook = home.path().join(".claude/hooks/aegis-pre-tool-use.sh");
        fs::write(&hook, "managed").unwrap();
        match fixture {
            "malformed" => {
                fs::write(home.path().join(".bashrc"), "# >>> aegis shell setup >>>\n").unwrap()
            }
            "settings-link" => std::os::unix::fs::symlink(
                outside.path().join("target"),
                home.path().join(".claude/settings.json"),
            )
            .unwrap(),
            _ => std::os::unix::fs::symlink(
                outside.path().join("target"),
                home.path().join(".zshrc"),
            )
            .unwrap(),
        }
        assert!(!uninstall(home.path(), &[]).status.success());
        assert!(hook.exists());
        assert_eq!(
            fs::read_to_string(outside.path().join("target")).unwrap(),
            "keep"
        );
    }
}

#[test]
fn uninstall_data_purge_does_not_follow_descendant_symlinks() {
    let home = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("backup"), "keep").unwrap();
    fs::create_dir_all(home.path().join(".aegis")).unwrap();
    std::os::unix::fs::symlink(outside.path(), home.path().join(".aegis/snapshots")).unwrap();
    assert!(uninstall(home.path(), &["--purge-data"]).status.success());
    assert!(!home.path().join(".aegis").exists());
    assert_eq!(
        fs::read_to_string(outside.path().join("backup")).unwrap(),
        "keep"
    );
}

#[test]
fn uninstall_reports_that_project_local_hooks_need_manual_cleanup() {
    let home = TempDir::new().unwrap();
    let output = uninstall(home.path(), &["--channel", "npm"]);
    assert!(output.status.success());
    let message = String::from_utf8_lossy(&output.stdout);
    assert!(message.contains("install-hooks --local"), "{message}");
    assert!(message.contains("manual cleanup"), "{message}");
}

#[test]
fn uninstall_detects_homebrew_through_a_symlink_that_matches_the_curl_bindir() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let cellar_binary = temp.path().join("Cellar/aegis/1.0/bin/aegis");
    fs::create_dir_all(cellar_binary.parent().unwrap()).unwrap();
    fs::copy(support::aegis_bin(), &cellar_binary).unwrap();
    let bindir = temp.path().join("bin");
    fs::create_dir_all(&bindir).unwrap();
    let link = bindir.join("aegis");
    std::os::unix::fs::symlink(&cellar_binary, &link).unwrap();

    let output = Command::new(&link)
        .env_clear()
        .env("HOME", &home)
        .env("AEGIS_BINDIR", &bindir)
        .args(["uninstall"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let message = String::from_utf8_lossy(&output.stdout);
    assert!(message.contains("brew uninstall aegis"), "{message}");
    assert!(!message.contains("rm -- "), "{message}");
    assert!(link.exists() && cellar_binary.exists());
}
