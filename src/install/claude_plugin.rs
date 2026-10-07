//! Detection of the Aegis Claude Code plugin (#500, ADR-047).
//!
//! The Claude Code plugin registers the same Aegis `Hook`s as `aegis
//! install-hooks --claude-code`. When it is enabled and installed, the
//! installer leaves `settings.json` alone so `aegis hook` does not run twice
//! per command.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// `enabledPlugins` keys that name the Aegis Claude Code plugin: the community
/// catalog and the repository's own marketplace. A wildcard such as `aegis@*`
/// would let a foreign plugin named `aegis` switch the settings hooks off.
const AEGIS_PLUGIN_KEYS: [&str; 2] = ["aegis@claude-community", "aegis@aegis-shellguard"];

/// Whether the Aegis Claude Code plugin registers its `Hook`s.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClaudePluginState {
    /// No Aegis key is enabled in `enabledPlugins`.
    Off,
    /// An Aegis key is enabled, but `installed_plugins.json` records no
    /// existing install directory for it, so Claude Code loads no plugin
    /// `Hook`. Synced dotfiles produce this on a machine that never added the
    /// marketplace.
    EnabledNotInstalled,
    /// An Aegis key is enabled and installed: the plugin registers the
    /// `Hook`s.
    Active,
}

/// Plugin state for a session in `cwd`. Scopes are read with Claude Code's
/// precedence: `<cwd>/.claude/settings.local.json`, then
/// `<cwd>/.claude/settings.json`, then `~/.claude/settings.json`. The first
/// scope that sets a key decides it. A missing or unparsable settings file
/// sets nothing. Used by `aegis status`, which reports what this user's
/// session registers.
pub(crate) fn claude_code_plugin_state(home: Option<&Path>, cwd: &Path) -> ClaudePluginState {
    plugin_state_in(home, &settings_scopes(home, cwd))
}

/// Plugin state from `~/.claude/settings.json` alone. A global install writes
/// hooks that cover every project, so a project scope must not decide it: a
/// repo that enables the plugin would leave every other project unguarded,
/// and a repo that disables it would make every other project run `aegis
/// hook` twice.
pub(crate) fn claude_code_plugin_state_for_user(home: Option<&Path>) -> ClaudePluginState {
    match home {
        Some(home_dir) => plugin_state_in(home, &[home_dir.join(".claude/settings.json")]),
        None => ClaudePluginState::Off,
    }
}

/// Plugin state from the shared project settings:
/// `<cwd>/.claude/settings.json`, then `~/.claude/settings.json`. A `--local`
/// install writes that committed file for every teammate, so the personal
/// `settings.local.json` must not decide it.
pub(crate) fn claude_code_plugin_state_for_project(
    home: Option<&Path>,
    cwd: &Path,
) -> ClaudePluginState {
    let mut scopes = vec![cwd.join(".claude/settings.json")];
    if let Some(home) = home {
        scopes.push(home.join(".claude/settings.json"));
    }
    plugin_state_in(home, &scopes)
}

/// The plugin is active only when a key is enabled and that same key is
/// installed. A key that is enabled but not installed registers nothing, so
/// treating it as active would leave Bash unguarded; a duplicate `Hook` is
/// harmless by comparison (ADR-047).
fn plugin_state_in(home: Option<&Path>, scopes: &[PathBuf]) -> ClaudePluginState {
    let scopes = scopes
        .iter()
        .filter_map(|path| read_enabled_plugins(path))
        .collect::<Vec<_>>();
    let enabled = AEGIS_PLUGIN_KEYS
        .iter()
        .filter(|key| {
            scopes
                .iter()
                .find_map(|plugins| plugins.get(**key))
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    if enabled.is_empty() {
        return ClaudePluginState::Off;
    }
    let installed = home.and_then(read_installed_plugins);
    if enabled.iter().any(|key| {
        installed
            .as_ref()
            .is_some_and(|plugins| has_install_dir(plugins, key))
    }) {
        ClaudePluginState::Active
    } else {
        ClaudePluginState::EnabledNotInstalled
    }
}

/// The `plugins` map of `~/.claude/plugins/installed_plugins.json`, or `None`
/// when the file is missing, unreadable, or malformed.
fn read_installed_plugins(home: &Path) -> Option<serde_json::Map<String, Value>> {
    let raw = fs::read_to_string(home.join(".claude/plugins/installed_plugins.json")).ok()?;
    let installed: Value = serde_json::from_str(&raw).ok()?;
    installed.get("plugins")?.as_object().cloned()
}

/// True when `key` has at least one install record whose `installPath` is an
/// existing directory.
fn has_install_dir(plugins: &serde_json::Map<String, Value>, key: &str) -> bool {
    plugins
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|record| record.get("installPath").and_then(Value::as_str))
        .any(|path| Path::new(path).is_dir())
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
/// PreToolUse entry counts: a second SessionStart notice is harmless. A
/// plugin that is enabled but not installed registers nothing, so it counts
/// as off.
pub(crate) fn claude_hook_registration(home: Option<&Path>, cwd: &Path) -> ClaudeHookRegistration {
    let plugin = claude_code_plugin_state(home, cwd) == ClaudePluginState::Active;
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
