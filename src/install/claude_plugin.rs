use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

const AEGIS_PLUGIN_KEYS: [&str; 2] = ["aegis@claude-community", "aegis@aegis-shellguard"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClaudePluginState {
    Off,
    EnabledNotInstalled,
    UnknownFormat,
    Active,
}

pub(crate) fn claude_code_plugin_state(home: Option<&Path>, cwd: &Path) -> ClaudePluginState {
    plugin_state_in(home, &settings_scopes(home, cwd))
}

pub(crate) fn claude_code_plugin_state_for_user(home: Option<&Path>) -> ClaudePluginState {
    match home {
        Some(home_dir) => plugin_state_in(home, &[home_dir.join(".claude/settings.json")]),
        None => ClaudePluginState::Off,
    }
}

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
    let plugins = match home.map_or(InstalledPlugins::Missing, read_installed_plugins) {
        InstalledPlugins::Missing => return ClaudePluginState::EnabledNotInstalled,
        InstalledPlugins::UnknownFormat => return ClaudePluginState::UnknownFormat,
        InstalledPlugins::Known(plugins) => plugins,
    };
    if enabled.iter().any(|key| has_install_dir(&plugins, key)) {
        ClaudePluginState::Active
    } else {
        ClaudePluginState::EnabledNotInstalled
    }
}

enum InstalledPlugins {
    Missing,
    UnknownFormat,
    Known(serde_json::Map<String, Value>),
}

const INSTALLED_PLUGINS_FORMAT_VERSION: u64 = 2;

fn read_installed_plugins(home: &Path) -> InstalledPlugins {
    let Some(installed) = fs::read_to_string(home.join(".claude/plugins/installed_plugins.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
    else {
        return InstalledPlugins::Missing;
    };
    if installed.get("version").and_then(Value::as_u64) != Some(INSTALLED_PLUGINS_FORMAT_VERSION) {
        return InstalledPlugins::UnknownFormat;
    }
    InstalledPlugins::Known(
        installed
            .get("plugins")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default(),
    )
}

fn has_install_dir(plugins: &serde_json::Map<String, Value>, key: &str) -> bool {
    plugins
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|record| record.get("installPath").and_then(Value::as_str))
        .any(|path| Path::new(path).is_dir())
}

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

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ClaudeHookRegistration {
    Plugin,
    Settings,
    None,
    Duplicate(Vec<PathBuf>),
    Unknown,
}

pub(crate) fn claude_hook_registration(home: Option<&Path>, cwd: &Path) -> ClaudeHookRegistration {
    let plugin = match claude_code_plugin_state(home, cwd) {
        ClaudePluginState::UnknownFormat => return ClaudeHookRegistration::Unknown,
        ClaudePluginState::Active => true,
        ClaudePluginState::Off | ClaudePluginState::EnabledNotInstalled => false,
    };
    let settings: Vec<PathBuf> = settings_scopes(home, cwd)
        .into_iter()
        .filter(|path| has_aegis_managed_pre_tool_use(path))
        .collect();
    match (plugin, settings.is_empty()) {
        (true, false) => ClaudeHookRegistration::Duplicate(settings),
        (true, true) => ClaudeHookRegistration::Plugin,
        (false, false) => ClaudeHookRegistration::Settings,
        (false, true) => ClaudeHookRegistration::None,
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
