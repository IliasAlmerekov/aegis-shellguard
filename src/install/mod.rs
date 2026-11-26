mod claude;
pub(crate) mod claude_plugin;
mod codex;
mod error;
mod hook;
mod shell;
pub(crate) mod uninstall;

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

pub(crate) use error::InstallError;
pub(crate) use hook::run_hook;
pub(crate) use shell::run_setup_shell;

pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(crate) fn resolved_aegis_bin() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.to_str().map(str::to_owned))
        .unwrap_or_else(|| "aegis".to_string())
}

fn executable_content_is_current(existing: &str, mode: u32, expected_content: &str) -> bool {
    existing == expected_content && mode & 0o777 == 0o755
}

pub(crate) fn write_executable(path: &Path, content: &str) -> Result<InstallOutcome, InstallError> {
    use std::os::unix::fs::PermissionsExt;

    match fs::read_to_string(path) {
        Ok(existing) => {
            let metadata = fs::metadata(path).map_err(InstallError::io("failed to stat", path))?;
            if executable_content_is_current(&existing, metadata.permissions().mode(), content) {
                return Ok(InstallOutcome::AlreadyPresent);
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(InstallError::Io {
                action: "failed to read",
                path: path.to_path_buf(),
                source,
            });
        }
    }

    if path.parent().is_none() {
        return Err(InstallError::InvalidPath {
            path: path.to_path_buf(),
            reason: "has no parent",
        });
    }
    aegis_config::write_atomic(path, content.as_bytes(), Some(0o755))
        .map_err(InstallError::io("failed to install", path))?;
    Ok(InstallOutcome::Installed)
}

pub(crate) fn run_install(args: &super::InstallArgs) -> i32 {
    let mut exit = 0;
    let selection = install_target_selection(args);

    if selection.includes_claude() {
        exit = match claude::run_claude_install(!args.local) {
            ClaudeInstallResult::SkippedClaudeCodePlugin => {
                out!("Claude Code: skipped (Claude Code plugin aegis is enabled)");
                exit
            }
            ClaudeInstallResult::PluginNotInstalled(result) => {
                let exit = report_claude_settings_install(result, exit);
                out!(
                    "Claude Code: plugin aegis is enabled but not installed; \
                     installed settings hooks instead"
                );
                exit
            }
            ClaudeInstallResult::PluginFormatUnknown(result) => {
                let exit = report_claude_settings_install(result, exit);
                out!(
                    "Claude Code: plugin aegis is enabled but installed_plugins.json has an \
                     unknown format; installed settings hooks instead"
                );
                exit
            }
            ClaudeInstallResult::Settings(result) => report_claude_settings_install(result, exit),
        };
    }

    if selection.includes_codex() {
        match codex::run_codex_install() {
            AgentInstallResult::Installed => out!("Codex: hooks installed"),
            AgentInstallResult::AlreadyPresent => {
                out!("Codex: hooks already present, skipping")
            }
            AgentInstallResult::Skipped => {
                out!("Codex: skipped (agent directory not present)")
            }
            AgentInstallResult::Error(err) => {
                err!("error: failed to install Codex hooks: {err}");
                exit = super::EXIT_INTERNAL;
            }
        }
    }

    exit
}

fn report_claude_settings_install(result: AgentInstallResult, exit: i32) -> i32 {
    match result {
        AgentInstallResult::Installed => out!("Claude Code: hook installed"),
        AgentInstallResult::AlreadyPresent => {
            out!("Claude Code: hook already present, skipping")
        }
        AgentInstallResult::Skipped => {
            out!("Claude Code: skipped (agent directory not present)")
        }
        AgentInstallResult::Error(err) => {
            err!("error: failed to install Claude Code hook: {err}");
            return super::EXIT_INTERNAL;
        }
    }
    exit
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InstallTargetSelection {
    All,
    ClaudeCode,
    Codex,
}

impl InstallTargetSelection {
    fn includes_claude(self) -> bool {
        matches!(self, Self::All | Self::ClaudeCode)
    }

    fn includes_codex(self) -> bool {
        matches!(self, Self::All | Self::Codex)
    }
}

fn install_target_selection(args: &super::InstallArgs) -> InstallTargetSelection {
    if args.all || (!args.claude_code && !args.codex) {
        InstallTargetSelection::All
    } else if args.claude_code {
        InstallTargetSelection::ClaudeCode
    } else {
        InstallTargetSelection::Codex
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InstallOutcome {
    Installed,
    AlreadyPresent,
    Skipped,
}

#[derive(Debug)]
pub(crate) enum AgentInstallResult {
    Installed,
    AlreadyPresent,
    Skipped,
    Error(InstallError),
}

#[derive(Debug)]
pub(crate) enum ClaudeInstallResult {
    SkippedClaudeCodePlugin,
    PluginNotInstalled(AgentInstallResult),
    PluginFormatUnknown(AgentInstallResult),
    Settings(AgentInstallResult),
}

impl AgentInstallResult {
    fn from_result(result: Result<InstallOutcome, InstallError>) -> Self {
        match result {
            Ok(InstallOutcome::Installed) => Self::Installed,
            Ok(InstallOutcome::AlreadyPresent) => Self::AlreadyPresent,
            Ok(InstallOutcome::Skipped) => Self::Skipped,
            Err(err) => Self::Error(err),
        }
    }
}

pub(crate) fn agent_dir_exists(agent_dir: &Path) -> Result<bool, InstallError> {
    if !agent_dir.exists() {
        return Ok(false);
    }

    if agent_dir.is_dir() {
        return Ok(true);
    }

    Err(InstallError::NotADirectory {
        path: agent_dir.to_path_buf(),
    })
}

pub(crate) fn combine_outcomes(lhs: InstallOutcome, rhs: InstallOutcome) -> InstallOutcome {
    if matches!(lhs, InstallOutcome::Installed) || matches!(rhs, InstallOutcome::Installed) {
        InstallOutcome::Installed
    } else if matches!(lhs, InstallOutcome::Skipped) || matches!(rhs, InstallOutcome::Skipped) {
        InstallOutcome::Skipped
    } else {
        InstallOutcome::AlreadyPresent
    }
}

pub(crate) fn is_aegis_managed_bash_command(command: &str) -> bool {
    if command == "aegis hook" {
        return true;
    }
    let Some(basename) = Path::new(command).file_name().and_then(OsStr::to_str) else {
        return false;
    };
    basename == "aegis-rewrite.sh" || basename == "aegis-pre-tool-use.sh"
}

pub(crate) fn is_aegis_managed_session_start_command(command: &str) -> bool {
    Path::new(command)
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|basename| basename == "aegis-session-start.sh")
}

pub(crate) fn prune_aegis_managed_hooks(
    entries: &mut Vec<Value>,
    section_label: &str,
    canonical_matcher: &str,
    canonical_command: &str,
    is_managed: impl Fn(&str) -> bool,
) -> Result<(bool, bool), InstallError> {
    let mut pruned_any = false;
    let mut canonical_present = false;
    let mut drop_indices: Vec<usize> = Vec::new();

    for (idx, entry) in entries.iter_mut().enumerate() {
        let Some(entry_obj) = entry.as_object_mut() else {
            continue;
        };

        let matcher = entry_obj
            .get("matcher")
            .and_then(Value::as_str)
            .unwrap_or("");
        let matcher_is_canonical = matcher == canonical_matcher;

        let Some(hooks) = entry_obj.get_mut("hooks").and_then(Value::as_array_mut) else {
            if matcher_is_canonical {
                return Err(InstallError::InvalidStructure(format!(
                    "{section_label} matching {canonical_matcher} must contain a hooks array"
                )));
            }
            continue;
        };

        if matcher_is_canonical {
            for hook in hooks.iter() {
                let hook_obj = hook.as_object().ok_or_else(|| {
                    InstallError::InvalidStructure(format!(
                        "{section_label} matching {canonical_matcher} hooks must contain objects"
                    ))
                })?;
                hook_obj
                    .get("type")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        InstallError::InvalidStructure(format!(
                            "{section_label} matching {canonical_matcher} hook must contain a string type"
                        ))
                    })?;
                hook_obj
                    .get("command")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        InstallError::InvalidStructure(format!(
                            "{section_label} matching {canonical_matcher} hook must contain a string command"
                        ))
                    })?;
            }
        }

        let before = hooks.len();
        let mut found_canonical_here = false;
        hooks.retain(|hook| {
            let Some(command) = hook
                .as_object()
                .and_then(|h| h.get("command"))
                .and_then(|c| c.as_str())
            else {
                return true;
            };
            if matcher_is_canonical && command == canonical_command {
                found_canonical_here = true;
                return true;
            }
            !is_managed(command)
        });

        if hooks.len() < before {
            pruned_any = true;
        }
        if found_canonical_here {
            canonical_present = true;
        }
        if before > 0 && hooks.is_empty() {
            drop_indices.push(idx);
        }
    }

    for idx in drop_indices.into_iter().rev() {
        entries.remove(idx);
    }

    Ok((pruned_any, canonical_present))
}

pub(crate) fn load_settings(path: &Path) -> Result<Value, InstallError> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Value::Object(Map::new()));
        }
        Err(source) => {
            return Err(InstallError::Io {
                action: "failed to read",
                path: path.to_path_buf(),
                source,
            });
        }
    };

    if raw.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }

    let value: Value = serde_json::from_str(&raw).map_err(|source| InstallError::ParseJson {
        path: path.to_path_buf(),
        source,
    })?;

    if value.is_object() {
        Ok(value)
    } else {
        Err(InstallError::NotJsonObject {
            path: path.to_path_buf(),
        })
    }
}

pub(crate) fn write_settings_atomically(path: &Path, settings: &Value) -> Result<(), InstallError> {
    let parent = path.parent().ok_or_else(|| InstallError::InvalidPath {
        path: path.to_path_buf(),
        reason: "does not have a parent directory",
    })?;

    fs::create_dir_all(parent).map_err(InstallError::io("failed to create", parent))?;

    let mut bytes =
        serde_json::to_vec_pretty(settings).map_err(|source| InstallError::SerializeJson {
            action: "failed to serialize JSON for",
            path: path.to_path_buf(),
            source,
        })?;
    bytes.push(b'\n');

    aegis_config::write_atomic(path, &bytes, None)
        .map_err(InstallError::io("failed to replace", path))
}

pub(crate) fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("USERPROFILE")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
}

pub(crate) fn settings_path_global(home_dir: Option<&Path>) -> Result<PathBuf, InstallError> {
    let home_dir = home_dir.ok_or(InstallError::HomeNotSet)?;
    Ok(home_dir.join(".claude/settings.json"))
}

pub(crate) fn settings_path_local(cwd: &Path) -> PathBuf {
    cwd.join(".claude/settings.json")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use tempfile::TempDir;

    #[test]
    fn write_executable_reports_a_path_it_cannot_read() {
        let temp = TempDir::new().unwrap();

        let error = write_executable(temp.path(), "#!/bin/sh\n").unwrap_err();

        assert!(matches!(
            error,
            InstallError::Io {
                action: "failed to read",
                ..
            }
        ));
    }

    #[test]
    fn resolved_aegis_bin_is_non_empty() {
        let bin = resolved_aegis_bin();
        assert!(!bin.is_empty(), "resolved aegis bin must never be empty");
    }

    #[test]
    fn agent_dir_exists_reports_not_a_directory_for_a_regular_file() {
        let dir = TempDir::new().expect("temp dir");
        let file = dir.path().join(".claude");
        fs::write(&file, "x").expect("write file");

        let err = agent_dir_exists(&file).expect_err("a file is not an agent directory");

        assert!(matches!(&err, InstallError::NotADirectory { path } if *path == file));
        assert_eq!(
            err.to_string(),
            format!("{} exists but is not a directory", file.display())
        );
    }

    #[test]
    fn write_executable_normalizes_mode_to_0755() {
        let dir = TempDir::new().expect("temp dir");
        let hook_path = dir.path().join("hook.sh");
        fs::write(&hook_path, "#!/bin/sh\nold\n").expect("write hook");
        fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o644))
            .expect("set permissions");

        let outcome = write_executable(&hook_path, "#!/bin/sh\nnew\n").expect("install");

        assert_eq!(
            outcome,
            InstallOutcome::Installed,
            "mismatched content must be rewritten"
        );
        let mode = fs::metadata(&hook_path)
            .expect("stat hook")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o755, "installed hook must normalize to 0755");

        let again = write_executable(&hook_path, "#!/bin/sh\nnew\n").expect("reinstall");
        assert_eq!(again, InstallOutcome::AlreadyPresent);
    }

    #[test]
    fn install_settings_path_uses_local_cwd_by_default() {
        let cwd = TempDir::new().expect("temp dir");

        let path = settings_path_local(cwd.path());
        assert_eq!(path, cwd.path().join(".claude/settings.json"));
    }

    #[test]
    fn install_settings_path_uses_home_for_global() {
        let home = TempDir::new().expect("home dir");

        let path = settings_path_global(Some(home.path())).expect("global path");
        assert_eq!(path, home.path().join(".claude/settings.json"));
    }

    #[test]
    fn load_settings_returns_empty_object_for_missing_file() {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("nonexistent.json");

        let result = load_settings(&path).expect("missing file must return Ok");
        assert!(
            result.is_object() && result.as_object().unwrap().is_empty(),
            "missing settings file must yield an empty JSON object, got: {result:?}"
        );
    }

    #[test]
    fn load_settings_returns_error_for_invalid_json() {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("bad.json");
        fs::write(&path, "not json").unwrap();

        let err = load_settings(&path).unwrap_err();
        assert!(
            err.to_string().contains("failed to parse"),
            "invalid JSON must produce a parse error, got: {err}"
        );
    }

    #[test]
    fn write_settings_atomically_creates_file_with_correct_content() {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("settings.json");
        let settings = serde_json::json!({"key": "value"});

        write_settings_atomically(&path, &settings).expect("must succeed");

        let written: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["key"], "value");
    }

    #[test]
    fn write_settings_atomically_replaces_existing_file() {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"old": true}"#).unwrap();

        let new_settings = serde_json::json!({"new": true});
        write_settings_atomically(&path, &new_settings).expect("must succeed");

        let written: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["new"], true);
        assert!(written.get("old").is_none());
    }

    #[test]
    fn install_target_selection_defaults_to_all_when_no_target_flags_are_set() {
        let args = crate::InstallArgs {
            local: false,
            all: false,
            claude_code: false,
            codex: false,
        };

        assert_eq!(install_target_selection(&args), InstallTargetSelection::All);
    }

    #[test]
    fn install_target_selection_honors_explicit_target_flags() {
        let claude_only = crate::InstallArgs {
            local: false,
            all: false,
            claude_code: true,
            codex: false,
        };
        assert_eq!(
            install_target_selection(&claude_only),
            InstallTargetSelection::ClaudeCode
        );

        let codex_only = crate::InstallArgs {
            local: false,
            all: false,
            claude_code: false,
            codex: true,
        };
        assert_eq!(
            install_target_selection(&codex_only),
            InstallTargetSelection::Codex
        );

        let all_targets = crate::InstallArgs {
            local: false,
            all: true,
            claude_code: false,
            codex: true,
        };
        assert_eq!(
            install_target_selection(&all_targets),
            InstallTargetSelection::All
        );
    }
}
