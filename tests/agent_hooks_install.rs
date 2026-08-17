// Integration tests for install/uninstall flows, split from agent_hooks.rs to
// keep both files within the 800-line budget (M5.1 quality gate, Task 1). The
// script runners and JSON probes are shared with agent_hooks.rs via
// support::agent_hooks rather than duplicated here.

mod support;

use std::fs;

use tempfile::TempDir;

use support::agent_hooks::{
    json_contains_command, prepare_agent_dirs, read_json, run_script, run_script_with_env,
};

#[test]
fn uninstall_prunes_claude_and_codex_hook_registrations() {
    let home = TempDir::new().unwrap();
    prepare_agent_dirs(home.path(), true, true);
    let rc_file = home.path().join(".bashrc");
    fs::write(&rc_file, "export FOO=bar\n").unwrap();

    let claude_settings = home.path().join(".claude").join("settings.json");
    fs::write(
        &claude_settings,
        serde_json::json!({
            "theme": "dark",
            "hooks": {
                "PreToolUse": [
                    {
                        "matcher": "Bash",
                        "hooks": [
                            { "type": "command", "command": "echo user-keep" }
                        ]
                    }
                ]
            }
        })
        .to_string(),
    )
    .unwrap();

    let install_output = run_script("agent-setup.sh", home.path(), &["--all"], None);
    assert!(install_output.status.success());

    let codex_hooks = home.path().join(".codex").join("hooks.json");
    let session_hook = home
        .path()
        .join(".codex")
        .join("hooks")
        .join("aegis-session-start.sh");
    let ptu_hook = home
        .path()
        .join(".codex")
        .join("hooks")
        .join("aegis-pre-tool-use.sh");
    let claude_shim = home
        .path()
        .join(".claude")
        .join("hooks")
        .join("aegis-pre-tool-use.sh");

    assert!(claude_settings.exists());
    assert!(codex_hooks.exists());
    assert!(session_hook.exists());
    assert!(ptu_hook.exists());
    assert!(
        claude_shim.exists(),
        "Claude shim must be materialized by install"
    );

    let claude_json = read_json(&claude_settings);
    let claude_shim_command = claude_shim.display().to_string();
    assert!(
        json_contains_command(&claude_json, "PreToolUse", &claude_shim_command),
        "Claude settings.json must register the absolute shim path before uninstall"
    );
    assert!(
        json_contains_command(&claude_json, "PreToolUse", "echo user-keep"),
        "Claude install must preserve unrelated user Bash hooks"
    );
    assert!(
        !json_contains_command(&claude_json, "PreToolUse", "aegis hook"),
        "Claude install must register the absolute shim, not the legacy bare command"
    );

    let rc_file_str = rc_file.display().to_string();
    let fake_bindir = home.path().join("bin");
    fs::create_dir_all(&fake_bindir).unwrap();
    let bindir_str = fake_bindir.display().to_string();
    let uninstall_output = run_script_with_env(
        "uninstall.sh",
        home.path(),
        &[],
        None,
        &[
            ("AEGIS_SHELL_RC", &rc_file_str),
            ("SHELL", "/bin/bash"),
            ("AEGIS_BINDIR", &bindir_str),
        ],
    );
    assert!(
        uninstall_output.status.success(),
        "uninstall must succeed: stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&uninstall_output.stdout),
        String::from_utf8_lossy(&uninstall_output.stderr)
    );

    assert!(!session_hook.exists());
    assert!(!ptu_hook.exists());
    assert!(
        !claude_shim.exists(),
        "uninstall must remove the absolute Claude hook shim"
    );

    let claude_json = read_json(&claude_settings);
    assert!(
        !json_contains_command(&claude_json, "PreToolUse", &claude_shim_command),
        "Claude settings.json must not retain the absolute shim registration"
    );
    assert!(
        !json_contains_command(&claude_json, "PreToolUse", "aegis hook"),
        "Claude settings.json must not retain the legacy bare aegis hook registration"
    );
    assert!(
        json_contains_command(&claude_json, "PreToolUse", "echo user-keep"),
        "uninstall must preserve unrelated user Bash hooks"
    );
    assert_eq!(
        claude_json["theme"], "dark",
        "uninstall must preserve unrelated top-level user settings"
    );

    let codex_session_command = session_hook.display().to_string();
    let codex_ptu_command = ptu_hook.display().to_string();
    let codex_json = read_json(&codex_hooks);
    assert!(
        !json_contains_command(&codex_json, "SessionStart", &codex_session_command),
        "Codex hooks.json must not retain the SessionStart registration"
    );
    assert!(
        !json_contains_command(&codex_json, "PreToolUse", &codex_ptu_command),
        "Codex hooks.json must not retain the PreToolUse registration"
    );

    assert_eq!(fs::read_to_string(&rc_file).unwrap(), "export FOO=bar\n");
}

#[test]
fn claude_install_migrates_legacy_aegis_hook_registration_to_absolute_shim() {
    let home = TempDir::new().unwrap();
    prepare_agent_dirs(home.path(), true, false);
    let claude_settings = home.path().join(".claude").join("settings.json");
    fs::write(
        &claude_settings,
        serde_json::json!({
            "hooks": {
                "PreToolUse": [
                    {
                        "matcher": "Bash",
                        "hooks": [{ "type": "command", "command": "aegis hook" }]
                    },
                    {
                        "matcher": "Bash",
                        "hooks": [{ "type": "command", "command": "echo user-keep" }]
                    }
                ]
            }
        })
        .to_string(),
    )
    .unwrap();

    let install_output = run_script("agent-setup.sh", home.path(), &["--claude-code"], None);
    assert!(
        install_output.status.success(),
        "claude install must succeed: stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&install_output.stdout),
        String::from_utf8_lossy(&install_output.stderr)
    );

    let claude_shim = home
        .path()
        .join(".claude")
        .join("hooks")
        .join("aegis-pre-tool-use.sh");
    assert!(
        claude_shim.exists(),
        "absolute shim must be materialized on disk"
    );

    let claude_json = read_json(&claude_settings);
    let shim_command = claude_shim.display().to_string();
    assert!(
        json_contains_command(&claude_json, "PreToolUse", &shim_command),
        "claude settings must register the absolute shim path; settings=\n{claude_json}"
    );
    assert!(
        !json_contains_command(&claude_json, "PreToolUse", "aegis hook"),
        "legacy bare `aegis hook` registration must be migrated away; settings=\n{claude_json}"
    );
    assert!(
        json_contains_command(&claude_json, "PreToolUse", "echo user-keep"),
        "unrelated user hook must survive the migration; settings=\n{claude_json}"
    );
}

#[test]
fn agent_setup_wrapper_delegates_to_binary_install_hooks_command() {
    let home = TempDir::new().unwrap();
    let fake_bin_dir = home.path().join("bin");
    let fake_aegis = fake_bin_dir.join("aegis");
    let args_log = home.path().join("agent-setup-args.log");

    fs::create_dir_all(&fake_bin_dir).unwrap();
    fs::write(
        &fake_aegis,
        format!(
            "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$*\" > '{}'\nprintf 'delegated from wrapper\\n'\n",
            args_log.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(&fake_aegis).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&fake_aegis, permissions).unwrap();
    }

    let fake_aegis_str = fake_aegis.display().to_string();
    let output = run_script_with_env(
        "agent-setup.sh",
        home.path(),
        &["--codex"],
        None,
        &[("AEGIS_BIN", &fake_aegis_str)],
    );

    assert!(
        output.status.success(),
        "wrapper must delegate successfully: stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(&args_log).unwrap(),
        "install-hooks --codex\n",
        "compatibility wrapper must forward its supported flags to aegis install-hooks"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "delegated from wrapper\n"
    );
    assert!(output.stderr.is_empty());
}

/// A third-party `SessionStart` entry may legitimately omit `matcher` — both
/// agents treat it as optional. Aegis must read such an entry as "not mine"
/// and install alongside it, not refuse the install: refusing leaves the
/// operator with no effective-state notice at all, which is the very gap M3a
/// closes (TASKS.md#M3a).
#[test]
fn codex_install_coexists_with_a_foreign_session_start_entry_without_a_matcher() {
    let home = TempDir::new().unwrap();
    prepare_agent_dirs(home.path(), false, true);
    let hooks_json = home.path().join(".codex").join("hooks.json");
    fs::write(
        &hooks_json,
        serde_json::json!({
            "hooks": {
                "SessionStart": [
                    { "hooks": [{ "type": "command", "command": "echo foreign-keep" }] }
                ]
            }
        })
        .to_string(),
    )
    .unwrap();

    let install_output = run_script("agent-setup.sh", home.path(), &["--codex"], None);
    assert!(
        install_output.status.success(),
        "codex install must survive a matcher-less foreign entry: stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&install_output.stdout),
        String::from_utf8_lossy(&install_output.stderr)
    );

    let session_shim = home
        .path()
        .join(".codex")
        .join("hooks")
        .join("aegis-session-start.sh");
    let json = read_json(&hooks_json);
    assert!(
        json_contains_command(&json, "SessionStart", &session_shim.display().to_string()),
        "codex hooks must register the session-start shim; hooks=\n{json}"
    );
    assert!(
        json_contains_command(&json, "SessionStart", "echo foreign-keep"),
        "the foreign matcher-less entry must survive the install; hooks=\n{json}"
    );
}

#[test]
fn claude_install_coexists_with_a_foreign_session_start_entry_without_a_matcher() {
    let home = TempDir::new().unwrap();
    prepare_agent_dirs(home.path(), true, false);
    let claude_settings = home.path().join(".claude").join("settings.json");
    fs::write(
        &claude_settings,
        serde_json::json!({
            "hooks": {
                "SessionStart": [
                    { "hooks": [{ "type": "command", "command": "echo foreign-keep" }] }
                ]
            }
        })
        .to_string(),
    )
    .unwrap();

    let install_output = run_script("agent-setup.sh", home.path(), &["--claude-code"], None);
    assert!(
        install_output.status.success(),
        "claude install must survive a matcher-less foreign entry: stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&install_output.stdout),
        String::from_utf8_lossy(&install_output.stderr)
    );

    let session_shim = home
        .path()
        .join(".claude")
        .join("hooks")
        .join("aegis-session-start.sh");
    let json = read_json(&claude_settings);
    assert!(
        json_contains_command(&json, "SessionStart", &session_shim.display().to_string()),
        "claude settings must register the session-start shim; settings=\n{json}"
    );
    assert!(
        json_contains_command(&json, "SessionStart", "echo foreign-keep"),
        "the foreign matcher-less entry must survive the install; settings=\n{json}"
    );
}
