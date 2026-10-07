//! Detection of the Aegis Claude Code plugin (#500, ADR-047).
//!
//! The Claude Code plugin registers the same Aegis `Hook`s as `aegis
//! install-hooks --claude-code`. When it is enabled, the installer leaves
//! `settings.json` alone so `aegis hook` does not run twice per command.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// `enabledPlugins` keys that name the Aegis Claude Code plugin: the community
/// catalog and the repository's own marketplace. A wildcard such as `aegis@*`
/// would let a foreign plugin named `aegis` switch the settings hooks off.
const AEGIS_PLUGIN_KEYS: [&str; 2] = ["aegis@claude-plugins-community", "aegis@aegis-shellguard"];

/// True when the effective `enabledPlugins` value of the Aegis Claude Code
/// plugin is `true` for a session in `cwd`. Scopes are read with Claude Code's
/// precedence: `<cwd>/.claude/settings.local.json`, then
/// `<cwd>/.claude/settings.json`, then `~/.claude/settings.json`. The first
/// scope that sets a key decides it. A missing or unparsable settings file
/// sets nothing. Used by `aegis status`, which reports what this user's
/// session registers.
pub(crate) fn claude_code_plugin_enabled(home: Option<&Path>, cwd: &Path) -> bool {
    plugin_enabled_in(&settings_scopes(home, cwd))
}

/// True when `~/.claude/settings.json` alone enables the Aegis Claude Code
/// plugin. A global install writes hooks that cover every project, so a
/// project scope must not decide it: a repo that enables the plugin would
/// leave every other project unguarded, and a repo that disables it would make
/// every other project run `aegis hook` twice.
pub(crate) fn claude_code_plugin_enabled_for_user(home: Option<&Path>) -> bool {
    home.is_some_and(|home| plugin_enabled_in(&[home.join(".claude/settings.json")]))
}

/// True when the shared project settings enable the Aegis Claude Code plugin:
/// `<cwd>/.claude/settings.json`, then `~/.claude/settings.json`. A `--local`
/// install writes that committed file for every teammate, so the personal
/// `settings.local.json` must not decide it.
pub(crate) fn claude_code_plugin_enabled_for_project(home: Option<&Path>, cwd: &Path) -> bool {
    let mut scopes = vec![cwd.join(".claude/settings.json")];
    if let Some(home) = home {
        scopes.push(home.join(".claude/settings.json"));
    }
    plugin_enabled_in(&scopes)
}

fn plugin_enabled_in(scopes: &[PathBuf]) -> bool {
    let scopes = scopes
        .iter()
        .filter_map(|path| read_enabled_plugins(path))
        .collect::<Vec<_>>();
    AEGIS_PLUGIN_KEYS.iter().any(|key| {
        scopes
            .iter()
            .find_map(|plugins| plugins.get(*key))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    })
}

/// Claude settings files in descending precedence.
fn settings_scopes(home: Option<&Path>, cwd: &Path) -> Vec<PathBuf> {
    let mut scopes = vec![
        cwd.join(".claude/settings.local.json"),
        cwd.join(".claude/settings.json"),
    ];
    if let Some(home) = home {
        scopes.push(home.join(".claude/settings.json"));
    }
    scopes
}

fn read_enabled_plugins(path: &Path) -> Option<serde_json::Map<String, Value>> {
    let raw = fs::read_to_string(path).ok()?;
    let settings: Value = serde_json::from_str(&raw).ok()?;
    settings.get("enabledPlugins")?.as_object().cloned()
}

/// Where the Aegis Claude Code `Hook`s are registered, as `aegis status`
/// reports it.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ClaudeHookRegistration {
    /// Only the enabled Claude Code plugin registers them.
    Plugin,
    /// Only an aegis-managed PreToolUse Bash entry in a settings file does.
    Settings,
    /// Nothing registers them.
    None,
    /// Both do, so `aegis hook` runs twice per command. Carries the settings
    /// file that holds the aegis-managed entry.
    Duplicate(PathBuf),
}

/// Classify the Claude `Hook` registration for `home` and `cwd`. Only the
/// PreToolUse entry counts: a second SessionStart notice is harmless.
pub(crate) fn claude_hook_registration(home: Option<&Path>, cwd: &Path) -> ClaudeHookRegistration {
    let plugin = claude_code_plugin_enabled(home, cwd);
    // Claude Code merges hooks from every settings scope, the personal
    // settings.local.json included, so an entry in any of them runs.
    let settings = settings_scopes(home, cwd)
        .into_iter()
        .find(|path| has_aegis_managed_pre_tool_use(path));
    match (plugin, settings) {
        (true, Some(path)) => ClaudeHookRegistration::Duplicate(path),
        (true, None) => ClaudeHookRegistration::Plugin,
        (false, Some(_)) => ClaudeHookRegistration::Settings,
        (false, None) => ClaudeHookRegistration::None,
    }
}

fn has_aegis_managed_pre_tool_use(path: &Path) -> bool {
    let Some(settings) = fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
    else {
        return false;
    };
    settings["hooks"]["PreToolUse"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry["hooks"].as_array())
        .flatten()
        .filter_map(|hook| hook["command"].as_str())
        .any(super::is_aegis_managed_bash_command)
}
