use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use serde_json::Value;

use super::{
    AgentInstallResult, InstallOutcome, combine_outcomes, load_settings, resolved_aegis_bin,
    shell_quote, write_executable, write_settings_atomically,
};

const CLAUDE_PRE_TOOL_USE_HOOK_SH: &str = include_str!("../../scripts/hooks/claude-code.sh");
const CLAUDE_SESSION_START_HOOK_SH: &str =
    include_str!("../../scripts/hooks/claude-session-start.sh");

pub(crate) fn run_claude_install(global: bool) -> AgentInstallResult {
    AgentInstallResult::from_result(run_install_inner(global))
}

fn run_install_inner(global: bool) -> Result<InstallOutcome, String> {
    if global {
        let home = super::home_dir();
        return run_global_claude_install_at_home(home.as_deref());
    }

    let cwd = std::env::current_dir()
        .map_err(|err| format!("failed to resolve current directory: {err}"))?;
    let settings_path = super::settings_path_local(&cwd);
    run_install_at_path(&settings_path)
}

fn run_global_claude_install_at_home(home_dir: Option<&Path>) -> Result<InstallOutcome, String> {
    let settings_path = super::settings_path_global(home_dir)?;
    let claude_dir = settings_path.parent().ok_or_else(|| {
        format!(
            "{} does not have a parent directory",
            settings_path.display()
        )
    })?;

    if !super::agent_dir_exists(claude_dir)? {
        return Ok(InstallOutcome::Skipped);
    }

    run_install_at_path(&settings_path)
}

fn run_install_at_path(settings_path: &Path) -> Result<InstallOutcome, String> {
    let mut settings = load_settings(settings_path)?;

    // The shim lives next to the settings file in `<settings_dir>/hooks/`.
    // Deriving the dir from the settings path keeps global and `--local`
    // installs on a single code path.
    let settings_dir = settings_path.parent().ok_or_else(|| {
        format!(
            "{} does not have a parent directory",
            settings_path.display()
        )
    })?;
    let hooks_dir = settings_dir.join("hooks");
    fs::create_dir_all(&hooks_dir)
        .map_err(|err| format!("failed to create {}: {err}", hooks_dir.display()))?;

    let shim_path = hooks_dir.join("aegis-pre-tool-use.sh");
    let shim_outcome = write_executable(&shim_path, &render_claude_pre_tool_use_hook())?;
    let session_shim_path = hooks_dir.join("aegis-session-start.sh");
    let session_shim_outcome = write_executable(&session_shim_path, CLAUDE_SESSION_START_HOOK_SH)?;

    // Resolve to an absolute path so the registered command is PATH-independent
    // even when install ran from a relative cwd (e.g. a project-local install).
    let hook_command = std::path::absolute(&shim_path)
        .map_err(|err| format!("failed to resolve absolute hook path: {err}"))?
        .to_str()
        .ok_or_else(|| "hook path is not valid UTF-8".to_string())?
        .to_owned();

    let session_hook_command = std::path::absolute(&session_shim_path)
        .map_err(|err| format!("failed to resolve absolute hook path: {err}"))?
        .to_str()
        .ok_or_else(|| "session-start hook path is not valid UTF-8".to_string())?
        .to_owned();

    let pre_tool_use_outcome = apply_installation(&mut settings, &hook_command)?;
    let session_start_outcome =
        apply_session_start_installation(&mut settings, &session_hook_command)?;
    let settings_outcome = combine_outcomes(pre_tool_use_outcome, session_start_outcome);
    if matches!(settings_outcome, InstallOutcome::Installed) {
        write_settings_atomically(settings_path, &settings)?;
    }

    Ok(combine_outcomes(
        combine_outcomes(shim_outcome, session_shim_outcome),
        settings_outcome,
    ))
}

fn apply_session_start_installation(
    settings: &mut Value,
    hook_command: &str,
) -> Result<InstallOutcome, String> {
    let root = settings
        .as_object_mut()
        .ok_or_else(|| "settings.json must contain a top-level JSON object".to_string())?;
    let hooks = root
        .entry("hooks".to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()))
        .as_object_mut()
        .ok_or_else(|| "settings.hooks must be a JSON object".to_string())?;
    let session_start = hooks
        .entry("SessionStart".to_string())
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| "settings.hooks.SessionStart must be a JSON array".to_string())?;

    for entry in session_start.iter() {
        let entry = entry.as_object().ok_or_else(|| {
            "settings.hooks.SessionStart entries must contain objects".to_string()
        })?;
        let matcher = entry
            .get("matcher")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                "settings.hooks.SessionStart entries must contain a string matcher".to_string()
            })?;
        let hooks = entry
            .get("hooks")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                format!("settings.hooks.SessionStart matching {matcher} entry must contain hooks")
            })?;
        for hook in hooks {
            let hook = hook.as_object().ok_or_else(|| {
                "settings.hooks.SessionStart hooks must contain objects".to_string()
            })?;
            let hook_type = hook.get("type").and_then(Value::as_str).ok_or_else(|| {
                "settings.hooks.SessionStart hooks must contain a string type".to_string()
            })?;
            let command = hook.get("command").and_then(Value::as_str).ok_or_else(|| {
                "settings.hooks.SessionStart hooks must contain a string command".to_string()
            })?;
            if hook_type == "command" && command == hook_command {
                return Ok(InstallOutcome::AlreadyPresent);
            }
        }
    }

    session_start.push(serde_json::json!({
        "matcher": "startup|resume",
        "hooks": [{ "type": "command", "command": hook_command }]
    }));
    Ok(InstallOutcome::Installed)
}

/// Materialize the Claude PreToolUse hook with `__AEGIS_BIN__` replaced by an
/// absolute, shell-quoted path to the Aegis binary. Mirrors the Codex renderer
/// so both shims stay behaviorally identical; only agent-specific comments
/// differ (see ADR-012 consequences).
fn render_claude_pre_tool_use_hook() -> String {
    CLAUDE_PRE_TOOL_USE_HOOK_SH.replace("__AEGIS_BIN__", &shell_quote(&resolved_aegis_bin()))
}

fn apply_installation(settings: &mut Value, hook_command: &str) -> Result<InstallOutcome, String> {
    let root = settings
        .as_object_mut()
        .ok_or_else(|| "settings.json must contain a top-level JSON object".to_string())?;

    let hooks = root
        .entry("hooks".to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    let hooks = hooks
        .as_object_mut()
        .ok_or_else(|| "settings.hooks must be a JSON object".to_string())?;

    let pre_tool_use = hooks
        .entry("PreToolUse".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    let pre_tool_use = pre_tool_use
        .as_array_mut()
        .ok_or_else(|| "settings.hooks.PreToolUse must be a JSON array".to_string())?;

    // Prune-then-add: remove every aegis-managed legacy Bash registration (the
    // bare `aegis hook`, the legacy `aegis-rewrite.sh` file, and any stale
    // `aegis-pre-tool-use.sh` at a different absolute path) while preserving the
    // canonical entry and any unrelated user hooks.
    let (pruned_any, canonical_present) =
        prune_aegis_managed_bash_hooks(pre_tool_use, hook_command)?;

    // Idempotent only when the canonical entry was already the sole aegis-managed
    // hook and nothing was pruned. Any pruning or a missing canonical entry means
    // the settings changed (or must change), so we report `Installed` and write.
    if canonical_present && !pruned_any {
        return Ok(InstallOutcome::AlreadyPresent);
    }
    if !canonical_present {
        pre_tool_use.push(serde_json::json!({
            "matcher": "Bash",
            "hooks": [
                {
                    "type": "command",
                    "command": hook_command
                }
            ]
        }));
    }

    Ok(InstallOutcome::Installed)
}

/// A Bash hook command that Aegis owns and may migrate away on install. The
/// predicate matches by **basename** for the file-backed forms so a moved or
/// renamed home directory still migrates; the bare two-token `aegis hook`
/// command is matched as a whole string (it is not a path). A user hook that
/// merely contains the substring `aegis` but is none of these is preserved.
fn is_aegis_managed_bash_command(command: &str) -> bool {
    if command == "aegis hook" {
        return true;
    }
    let Some(basename) = Path::new(command).file_name().and_then(OsStr::to_str) else {
        return false;
    };
    basename == "aegis-rewrite.sh" || basename == "aegis-pre-tool-use.sh"
}

/// Walk `PreToolUse`, and for each `matcher == "Bash"` entry drop hook objects
/// whose command is aegis-managed **except** the canonical `hook_command`. Drop
/// entries emptied by pruning. Returns `(pruned_any, canonical_present)`.
///
/// Malformed entries/hooks fail closed with the same typed errors as the
/// historical validation, so the existing malformed-input tests still hold.
fn prune_aegis_managed_bash_hooks(
    entries: &mut Vec<Value>,
    canonical_command: &str,
) -> Result<(bool, bool), String> {
    let mut pruned_any = false;
    let mut canonical_present = false;
    let mut drop_indices: Vec<usize> = Vec::new();

    for (idx, entry) in entries.iter_mut().enumerate() {
        let entry_obj = entry
            .as_object_mut()
            .ok_or_else(|| "settings.hooks.PreToolUse entries must contain objects".to_string())?;
        // Scope the matcher borrow so it ends before the mutable `hooks` borrow.
        let matcher_is_bash = {
            let matcher = entry_obj
                .get("matcher")
                .ok_or_else(|| {
                    "settings.hooks.PreToolUse entries must contain matcher".to_string()
                })?
                .as_str()
                .ok_or_else(|| {
                    "settings.hooks.PreToolUse entry matcher must be a string".to_string()
                })?;
            matcher == "Bash"
        };

        if !matcher_is_bash {
            continue;
        }

        let hooks = entry_obj
            .get_mut("hooks")
            .ok_or_else(|| {
                "settings.hooks.PreToolUse matching Bash entry must contain hooks".to_string()
            })?
            .as_array_mut()
            .ok_or_else(|| {
                "settings.hooks.PreToolUse matching Bash entry hooks must be an array".to_string()
            })?;

        // Validate every hook shape before pruning so malformed hooks fail
        // closed exactly as the historical validation did.
        for hook in hooks.iter() {
            let hook_obj = hook.as_object().ok_or_else(|| {
                "settings.hooks.PreToolUse matching Bash entry hooks must contain objects"
                    .to_string()
            })?;
            hook_obj
                .get("type")
                .ok_or_else(|| {
                    "settings.hooks.PreToolUse matching Bash hook must contain type".to_string()
                })?
                .as_str()
                .ok_or_else(|| {
                    "settings.hooks.PreToolUse matching Bash hook type must be a string".to_string()
                })?;
            hook_obj
                .get("command")
                .ok_or_else(|| {
                    "settings.hooks.PreToolUse matching Bash hook must contain command".to_string()
                })?
                .as_str()
                .ok_or_else(|| {
                    "settings.hooks.PreToolUse matching Bash hook command must be a string"
                        .to_string()
                })?;
        }

        let before = hooks.len();
        let mut found_canonical = false;
        hooks.retain(|hook| {
            let Some(command) = hook
                .as_object()
                .and_then(|h| h.get("command"))
                .and_then(|c| c.as_str())
            else {
                return true;
            };
            if command == canonical_command {
                found_canonical = true;
                return true;
            }
            // Keep user hooks; drop only aegis-managed legacy commands.
            !is_aegis_managed_bash_command(command)
        });

        if hooks.len() < before {
            pruned_any = true;
        }
        if found_canonical {
            canonical_present = true;
        }
        // Drop the entry only if pruning emptied a previously non-empty entry,
        // so an already-empty user entry is left untouched.
        if before > 0 && hooks.is_empty() {
            drop_indices.push(idx);
        }
    }

    // Remove emptied entries in reverse index order to keep indices valid.
    for idx in drop_indices.into_iter().rev() {
        entries.remove(idx);
    }

    Ok((pruned_any, canonical_present))
}

#[cfg(test)]
mod tests;
