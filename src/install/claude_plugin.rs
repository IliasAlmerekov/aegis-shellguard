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
/// plugin is `true`. Scopes are read with Claude Code's precedence:
/// `<cwd>/.claude/settings.local.json`, then `<cwd>/.claude/settings.json`,
/// then `~/.claude/settings.json`. The first scope that sets a key decides it.
/// A missing or unparsable settings file sets nothing.
pub(crate) fn claude_code_plugin_enabled(home: Option<&Path>, cwd: &Path) -> bool {
    let scopes = settings_scopes(home, cwd)
        .into_iter()
        .filter_map(|path| read_enabled_plugins(&path))
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
