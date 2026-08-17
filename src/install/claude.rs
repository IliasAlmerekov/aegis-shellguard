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

    // Prune-then-add: drop every aegis-managed SessionStart registration that
    // is not the canonical `startup|resume` one (including a stale entry under
    // another matcher that fires the notice twice), while preserving unrelated
    // user hooks.
    let (pruned_any, canonical_present) = super::prune_aegis_managed_hooks(
        session_start,
        "settings.hooks.SessionStart",
        "startup|resume",
        hook_command,
        super::is_aegis_managed_session_start_command,
    )?;

    // Idempotent only when the canonical entry was already the sole
    // aegis-managed hook and nothing was pruned.
    if canonical_present && !pruned_any {
        return Ok(InstallOutcome::AlreadyPresent);
    }
    if !canonical_present {
        session_start.push(serde_json::json!({
            "matcher": "startup|resume",
            "hooks": [{ "type": "command", "command": hook_command }]
        }));
    }

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
    // `aegis-pre-tool-use.sh` at a different absolute path or under a matcher
    // Aegis does not install) while preserving the canonical entry and any
    // unrelated user hooks.
    let (pruned_any, canonical_present) = super::prune_aegis_managed_hooks(
        pre_tool_use,
        "settings.hooks.PreToolUse",
        "Bash",
        hook_command,
        super::is_aegis_managed_bash_command,
    )?;

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

#[cfg(test)]
mod tests;
