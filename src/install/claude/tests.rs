//! Unit tests for the Claude Code installer.

use std::fs;
use std::os::unix::fs::PermissionsExt;

use super::*;
use tempfile::TempDir;

/// A fixed absolute command used by the JSON-only `apply_installation`
/// tests so they can exercise registration without touching the filesystem.
const TEST_HOOK_COMMAND: &str = "/tmp/aegis-hooks/aegis-pre-tool-use.sh";

#[test]
fn render_claude_pre_tool_use_hook_substitutes_absolute_binary_path() {
    let rendered = render_claude_pre_tool_use_hook();

    assert!(
        !rendered.contains("__AEGIS_BIN__"),
        "placeholder must be substituted at install time, got:\n{rendered}"
    );
    let expected = format!("AEGIS_BIN={}", shell_quote(&resolved_aegis_bin()));
    assert!(
        rendered.contains(&expected),
        "rendered hook must assign the shell-quoted absolute aegis path, got:\n{rendered}"
    );
    // The transparent-rewrite shim must not reintroduce jq/python3 parsing.
    assert!(!rendered.contains("python3 -"));
    assert!(!rendered.contains("jq -"));
    assert!(rendered.contains("exec \"${AEGIS_BIN}\" hook"));
}

#[test]
fn claude_install_materializes_pre_tool_use_shim() {
    let dir = TempDir::new().expect("temp dir");
    let settings_dir = dir.path().join(".claude");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let settings_path = settings_dir.join("settings.json");
    fs::write(&settings_path, "{}\n").expect("seed settings file");

    let outcome = run_install_at_path(&settings_path).expect("install");
    assert!(matches!(outcome, InstallOutcome::Installed));

    let shim = settings_dir.join("hooks").join("aegis-pre-tool-use.sh");
    assert!(shim.exists(), "shim must be materialized at the hooks dir");
    let content = fs::read_to_string(&shim).expect("read shim");
    assert!(
        !content.contains("__AEGIS_BIN__"),
        "placeholder must be substituted in the materialized shim"
    );
    let mode = fs::metadata(&shim).expect("stat shim").permissions().mode() & 0o777;
    assert_eq!(mode, 0o755, "materialized shim must be executable");
}

#[test]
fn claude_install_registers_absolute_hook_command() {
    let dir = TempDir::new().expect("temp dir");
    let settings_dir = dir.path().join(".claude");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let settings_path = settings_dir.join("settings.json");
    fs::write(&settings_path, "{}\n").expect("seed settings file");

    run_install_at_path(&settings_path).expect("install");

    let written = fs::read_to_string(&settings_path).expect("read settings");
    let parsed: Value = serde_json::from_str(&written).expect("parse settings");
    let command = parsed["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
        .as_str()
        .expect("command string");
    assert_ne!(
        command, "aegis hook",
        "must not register the PATH-dependent bare command"
    );
    let expected_shim = settings_dir.join("hooks").join("aegis-pre-tool-use.sh");
    assert_eq!(
        command,
        expected_shim.display().to_string(),
        "must register the absolute shim path"
    );
    assert!(
        command.starts_with('/'),
        "registered command must be absolute"
    );
}

#[test]
fn install_round_trip_writes_settings_file_atomically() {
    let dir = TempDir::new().expect("temp dir");
    let settings_dir = dir.path().join(".claude");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    let settings_path = settings_dir.join("settings.json");
    fs::write(&settings_path, "{}\n").expect("seed settings file");

    let outcome = run_install_at_path(&settings_path).expect("install");
    assert!(matches!(outcome, InstallOutcome::Installed));

    let written = fs::read_to_string(&settings_path).expect("read settings");
    let parsed: Value = serde_json::from_str(&written).expect("parse settings");
    let expected_shim = settings_dir.join("hooks").join("aegis-pre-tool-use.sh");
    let expected_session_shim = settings_dir.join("hooks").join("aegis-session-start.sh");
    assert_eq!(
        parsed,
        serde_json::json!({
            "hooks": {
                "PreToolUse": [
                    {
                        "matcher": "Bash",
                        "hooks": [
                            {
                                "type": "command",
                                "command": expected_shim.display().to_string()
                            }
                        ]
                    }
                ],
                "SessionStart": [
                    {
                        "matcher": "startup|resume",
                        "hooks": [
                            {
                                "type": "command",
                                "command": expected_session_shim.display().to_string()
                            }
                        ]
                    }
                ]
            }
        })
    );
}

#[test]
fn global_claude_install_skips_when_agent_dir_is_missing() {
    let home = TempDir::new().expect("home dir");

    let outcome = run_global_claude_install_at_home(Some(home.path())).expect("install");

    assert!(matches!(outcome, InstallOutcome::Skipped));
    assert!(!home.path().join(".claude/settings.json").exists());
}

#[test]
fn global_claude_install_errors_on_malformed_settings_json() {
    let home = TempDir::new().expect("home dir");
    let claude_dir = home.path().join(".claude");
    fs::create_dir_all(&claude_dir).expect("create claude dir");
    fs::write(claude_dir.join("settings.json"), "{not valid json").expect("write settings");

    let err = run_global_claude_install_at_home(Some(home.path()))
        .expect_err("malformed settings should error");

    assert!(err.contains(".claude/settings.json"));
}

#[test]
fn global_claude_install_errors_on_malformed_nested_bash_hook_entry() {
    let home = TempDir::new().expect("home dir");
    let claude_dir = home.path().join(".claude");
    fs::create_dir_all(&claude_dir).expect("create claude dir");
    fs::write(
        claude_dir.join("settings.json"),
        serde_json::json!({
            "hooks": {
                "PreToolUse": [
                    {
                        "matcher": "Bash",
                        "hooks": "not-an-array"
                    }
                ]
            }
        })
        .to_string(),
    )
    .expect("write settings");

    let err = run_global_claude_install_at_home(Some(home.path()))
        .expect_err("malformed nested bash hook should error");

    assert!(err.contains("settings.hooks.PreToolUse"));
}

#[test]
fn global_claude_install_errors_on_non_object_pre_tool_use_member() {
    let home = TempDir::new().expect("home dir");
    let claude_dir = home.path().join(".claude");
    fs::create_dir_all(&claude_dir).expect("create claude dir");
    fs::write(
        claude_dir.join("settings.json"),
        serde_json::json!({
            "hooks": {
                "PreToolUse": ["bad-entry"]
            }
        })
        .to_string(),
    )
    .expect("write settings");

    let err = run_global_claude_install_at_home(Some(home.path()))
        .expect_err("non-object pre-tool-use member should error");

    assert!(err.contains("settings.hooks.PreToolUse"));
}

#[test]
fn global_claude_install_errors_on_non_string_bash_matcher() {
    let home = TempDir::new().expect("home dir");
    let claude_dir = home.path().join(".claude");
    fs::create_dir_all(&claude_dir).expect("create claude dir");
    fs::write(
        claude_dir.join("settings.json"),
        serde_json::json!({
            "hooks": {
                "PreToolUse": [
                    {
                        "matcher": 7,
                        "hooks": []
                    }
                ]
            }
        })
        .to_string(),
    )
    .expect("write settings");

    let err = run_global_claude_install_at_home(Some(home.path()))
        .expect_err("non-string matcher should error");

    assert!(err.contains("settings.hooks.PreToolUse"));
}

#[test]
fn local_install_can_bootstrap_project_settings_when_missing() {
    let project = TempDir::new().expect("project dir");
    let settings_path = super::super::settings_path_local(project.path());

    let outcome = run_install_at_path(&settings_path).expect("install");

    assert!(matches!(outcome, InstallOutcome::Installed));
    assert!(settings_path.exists());
}

#[test]
fn install_is_idempotent_and_preserves_existing_entries() {
    let mut settings = serde_json::json!({
        "hooks": {
            "PreToolUse": [
                {
                    "matcher": "Bash",
                    "hooks": [
                        {
                            "type": "command",
                            "command": "echo keep"
                        }
                    ]
                }
            ]
        }
    });

    let outcome = apply_installation(&mut settings, TEST_HOOK_COMMAND).expect("first install");
    assert!(matches!(outcome, InstallOutcome::Installed));

    let pre_tool_use = settings["hooks"]["PreToolUse"]
        .as_array()
        .expect("PreToolUse array");
    assert_eq!(pre_tool_use.len(), 2);
    assert_eq!(
        pre_tool_use[1],
        serde_json::json!({
            "matcher": "Bash",
            "hooks": [
                {
                    "type": "command",
                    "command": TEST_HOOK_COMMAND
                }
            ]
        })
    );

    let outcome = apply_installation(&mut settings, TEST_HOOK_COMMAND).expect("second install");
    assert!(matches!(outcome, InstallOutcome::AlreadyPresent));
    assert_eq!(
        settings["hooks"]["PreToolUse"]
            .as_array()
            .expect("PreToolUse array")
            .len(),
        2
    );
}

#[test]
fn install_ignores_non_bash_hook_with_aegis_command() {
    let mut settings = serde_json::json!({
        "hooks": {
            "PreToolUse": [
                {
                    "matcher": "Git",
                    "hooks": [
                        {
                            "type": "command",
                            "command": "aegis hook"
                        }
                    ]
                }
            ]
        }
    });

    let outcome = apply_installation(&mut settings, TEST_HOOK_COMMAND).expect("install");
    assert!(matches!(outcome, InstallOutcome::Installed));
    assert_eq!(
        settings["hooks"]["PreToolUse"]
            .as_array()
            .expect("PreToolUse array")
            .len(),
        2
    );
}

#[test]
fn install_adds_hooks_tree_when_missing() {
    let mut settings = serde_json::json!({});

    let outcome = apply_installation(&mut settings, TEST_HOOK_COMMAND).expect("install");
    assert!(matches!(outcome, InstallOutcome::Installed));
    assert_eq!(
        settings,
        serde_json::json!({
            "hooks": {
                "PreToolUse": [
                    {
                        "matcher": "Bash",
                        "hooks": [
                            {
                                "type": "command",
                                "command": TEST_HOOK_COMMAND
                            }
                        ]
                    }
                ]
            }
        })
    );
}

/// True when any PreToolUse entry (any matcher) has a hook with the given
/// command. Used by the migration tests to assert legacy commands are gone.
fn any_hook_command(entries: &Value, command: &str) -> bool {
    entries.as_array().is_some_and(|arr| {
        arr.iter().any(|entry| {
            entry["hooks"]
                .as_array()
                .is_some_and(|hooks| hooks.iter().any(|hook| hook["command"] == command))
        })
    })
}

/// Count PreToolUse Bash entries that own the canonical aegis hook command.
fn aegis_entry_count(entries: &Value, command: &str) -> usize {
    entries
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|entry| {
                    entry["matcher"] == "Bash"
                        && entry["hooks"].as_array().is_some_and(|hooks| {
                            hooks.iter().any(|hook| hook["command"] == command)
                        })
                })
                .count()
        })
        .unwrap_or(0)
}

#[test]
fn claude_install_migrates_from_bare_aegis_hook() {
    let mut settings = serde_json::json!({
        "hooks": {
            "PreToolUse": [
                {
                    "matcher": "Bash",
                    "hooks": [
                        { "type": "command", "command": "aegis hook" }
                    ]
                }
            ]
        }
    });

    let outcome = apply_installation(&mut settings, TEST_HOOK_COMMAND).expect("install");
    assert!(matches!(outcome, InstallOutcome::Installed));

    let pre_tool_use = &settings["hooks"]["PreToolUse"];
    assert_eq!(
        aegis_entry_count(pre_tool_use, TEST_HOOK_COMMAND),
        1,
        "exactly one aegis-managed Bash entry must remain"
    );
    assert!(
        !any_hook_command(pre_tool_use, "aegis hook"),
        "legacy bare `aegis hook` registration must be migrated away"
    );
}

#[test]
fn claude_install_migrates_from_legacy_rewrite_script() {
    let legacy = "/home/u/.claude/hooks/aegis-rewrite.sh";
    let mut settings = serde_json::json!({
        "hooks": {
            "PreToolUse": [
                {
                    "matcher": "Bash",
                    "hooks": [
                        { "type": "command", "command": legacy }
                    ]
                }
            ]
        }
    });

    apply_installation(&mut settings, TEST_HOOK_COMMAND).expect("install");

    let pre_tool_use = &settings["hooks"]["PreToolUse"];
    assert_eq!(
        aegis_entry_count(pre_tool_use, TEST_HOOK_COMMAND),
        1,
        "the canonical absolute shim must be registered"
    );
    assert!(
        !any_hook_command(pre_tool_use, legacy),
        "legacy aegis-rewrite.sh registration must be migrated away"
    );
}

#[test]
fn claude_install_preserves_unrelated_user_bash_hook() {
    let mut settings = serde_json::json!({
        "hooks": {
            "PreToolUse": [
                {
                    "matcher": "Bash",
                    "hooks": [
                        { "type": "command", "command": "echo keep" }
                    ]
                }
            ]
        }
    });

    apply_installation(&mut settings, TEST_HOOK_COMMAND).expect("install");

    let pre_tool_use = &settings["hooks"]["PreToolUse"];
    assert!(
        any_hook_command(pre_tool_use, "echo keep"),
        "unrelated user Bash hook must be preserved"
    );
    assert_eq!(
        aegis_entry_count(pre_tool_use, TEST_HOOK_COMMAND),
        1,
        "exactly one aegis-managed Bash entry must be present"
    );
}

#[test]
fn claude_install_is_idempotent_after_migration() {
    let mut settings = serde_json::json!({
        "hooks": {
            "PreToolUse": [
                {
                    "matcher": "Bash",
                    "hooks": [
                        { "type": "command", "command": "aegis hook" }
                    ]
                }
            ]
        }
    });

    let first = apply_installation(&mut settings, TEST_HOOK_COMMAND).expect("first install");
    assert!(matches!(first, InstallOutcome::Installed));

    let second = apply_installation(&mut settings, TEST_HOOK_COMMAND).expect("second install");
    assert!(
        matches!(second, InstallOutcome::AlreadyPresent),
        "reinstall after migration must be idempotent"
    );

    let pre_tool_use = &settings["hooks"]["PreToolUse"];
    assert_eq!(
        aegis_entry_count(pre_tool_use, TEST_HOOK_COMMAND),
        1,
        "no duplicate aegis entries after reinstall"
    );
}

#[test]
fn claude_install_preserves_user_hook_that_merely_mentions_aegis() {
    let mut settings = serde_json::json!({
        "hooks": {
            "PreToolUse": [
                {
                    "matcher": "Bash",
                    "hooks": [
                        { "type": "command", "command": "aegis-lint --check" }
                    ]
                }
            ]
        }
    });

    apply_installation(&mut settings, TEST_HOOK_COMMAND).expect("install");

    let pre_tool_use = &settings["hooks"]["PreToolUse"];
    assert!(
        any_hook_command(pre_tool_use, "aegis-lint --check"),
        "a user hook that merely mentions aegis (basename not managed) must be preserved"
    );
    assert_eq!(
        aegis_entry_count(pre_tool_use, TEST_HOOK_COMMAND),
        1,
        "exactly one aegis-managed Bash entry must be present"
    );
}

#[test]
fn claude_session_start_install_is_idempotent_and_preserves_user_hooks() {
    let mut settings = serde_json::json!({
        "hooks": {
            "SessionStart": [{
                "matcher": "startup",
                "hooks": [{ "type": "command", "command": "echo keep" }]
            }]
        }
    });

    let first = apply_session_start_installation(&mut settings, "/tmp/aegis-session-start.sh")
        .expect("first install");
    assert!(matches!(first, InstallOutcome::Installed));
    let second = apply_session_start_installation(&mut settings, "/tmp/aegis-session-start.sh")
        .expect("second install");
    assert!(matches!(second, InstallOutcome::AlreadyPresent));

    let entries = settings["hooks"]["SessionStart"]
        .as_array()
        .expect("entries");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["hooks"][0]["command"], "echo keep");
}
