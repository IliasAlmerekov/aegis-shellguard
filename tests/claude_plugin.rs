//! Tests for the Claude Code plugin shipped under `plugins/aegis/` and listed
//! by the root `.claude-plugin/marketplace.json` (#500, ADR-047).
//!
//! The plugin hooks run as processes with a controlled `PATH`, `HOME`, and
//! `AEGIS_BIN`, the same way Claude Code runs them.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

/// CI markers the hooks consult; cleared so a developer's environment cannot
/// flip a hook into its CI-override branch.
const CI_MARKER_VARS: [&str; 9] = [
    "AEGIS_CI",
    "CI",
    "GITHUB_ACTIONS",
    "GITLAB_CI",
    "CIRCLECI",
    "BUILDKITE",
    "TRAVIS",
    "TF_BUILD",
    "JENKINS_URL",
];

const PLUGIN_PRE_TOOL_USE: &str = "plugins/aegis/hooks/aegis-pre-tool-use.sh";
const PLUGIN_SESSION_START: &str = "plugins/aegis/hooks/aegis-session-start.sh";

const UNAVAILABLE_DENY: &str = "aegis binary unavailable; refusing to run command unscanned";

fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn read_repo_file(relative: &str) -> String {
    let path = repo_path(relative);
    fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

fn assert_executable(path: &Path) {
    let mode = fs::metadata(path).unwrap().permissions().mode();
    assert_eq!(
        mode & 0o111,
        0o111,
        "{} must be executable (mode {mode:o})",
        path.display()
    );
}

#[test]
fn claude_plugin_hooks_match_templates() {
    for (plugin_hook, template) in [
        (
            "plugins/aegis/hooks/aegis-pre-tool-use.sh",
            "scripts/hooks/claude-code.sh",
        ),
        (
            "plugins/aegis/hooks/aegis-session-start.sh",
            "scripts/hooks/claude-session-start.sh",
        ),
    ] {
        let expected = read_repo_file(template).replace("__AEGIS_BIN__", "aegis");
        assert_eq!(
            read_repo_file(plugin_hook),
            expected,
            "{plugin_hook} drifted from {template}; regenerate it by replacing __AEGIS_BIN__ with aegis"
        );
        assert_executable(&repo_path(plugin_hook));
    }
}

/// A `PATH` directory holding only the tools the hook scripts call, so the
/// test controls whether `aegis` resolves. With `with_aegis` the test binary
/// is linked in as `aegis`.
fn tool_path(dir: &Path, with_aegis: bool) -> String {
    for tool in ["tr", "printenv", "cat", "grep"] {
        let source = ["/usr/bin", "/bin"]
            .iter()
            .map(|prefix| Path::new(prefix).join(tool))
            .find(|candidate| candidate.exists())
            .unwrap_or_else(|| panic!("{tool} not found in /usr/bin or /bin"));
        std::os::unix::fs::symlink(source, dir.join(tool)).unwrap();
    }
    if with_aegis {
        std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_aegis"), dir.join("aegis")).unwrap();
    }
    dir.display().to_string()
}

fn bash_path() -> &'static str {
    ["/usr/bin/bash", "/bin/bash"]
        .into_iter()
        .find(|candidate| Path::new(candidate).exists())
        .expect("bash is required for the hook tests")
}

fn run_hook(script: &Path, home: &Path, path: &str, stdin: &str) -> Output {
    let mut command = Command::new(bash_path());
    command
        .arg(script)
        .env_clear()
        .env("HOME", home)
        .env("PATH", path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for key in CI_MARKER_VARS {
        command.env_remove(key);
    }
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn bash_payload(command: &str) -> String {
    serde_json::json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": { "command": command }
    })
    .to_string()
}

/// Run `aegis install-hooks --claude-code` against a temp HOME with a
/// `~/.claude` directory, so the rendered shims land in `~/.claude/hooks/`.
fn install_claude_settings_hooks(home: &Path) {
    fs::create_dir_all(home.join(".claude")).unwrap();
    let install = Command::new(env!("CARGO_BIN_EXE_aegis"))
        .args(["install-hooks", "--claude-code"])
        .env("HOME", home)
        .current_dir(home)
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "install failed: {}",
        String::from_utf8_lossy(&install.stderr)
    );
}

#[test]
fn claude_plugin_hook_matches_installed_shim() {
    let home = TempDir::new().unwrap();
    install_claude_settings_hooks(home.path());
    let shim = home.path().join(".claude/hooks/aegis-pre-tool-use.sh");
    let bin = TempDir::new().unwrap();
    let path = tool_path(bin.path(), true);

    for command in [
        "aegis --command 'git status'",
        "ls -la",
        "rm -rf /",
        "aegis off",
    ] {
        let payload = bash_payload(command);
        let shim_out = run_hook(&shim, home.path(), &path, &payload);
        let plugin_out = run_hook(&repo_path(PLUGIN_PRE_TOOL_USE), home.path(), &path, &payload);
        assert!(shim_out.status.success() && plugin_out.status.success());
        assert_eq!(
            String::from_utf8_lossy(&plugin_out.stdout),
            String::from_utf8_lossy(&shim_out.stdout),
            "plugin hook and installed shim disagree for `{command}`"
        );
    }
}

#[test]
fn claude_plugin_hook_denies_without_binary() {
    let home = TempDir::new().unwrap();
    let bin = TempDir::new().unwrap();
    let path = tool_path(bin.path(), false);

    let output = run_hook(
        &repo_path(PLUGIN_PRE_TOOL_USE),
        home.path(),
        &path,
        &bash_payload("ls -la"),
    );

    assert_eq!(output.status.code(), Some(0));
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stdout["reason"], UNAVAILABLE_DENY);
    assert_eq!(stdout["hookSpecificOutput"]["permissionDecision"], "deny");
    assert_eq!(
        stdout["hookSpecificOutput"]["permissionDecisionReason"],
        UNAVAILABLE_DENY
    );
}

#[test]
fn claude_session_start_reports_missing_binary() {
    let home = TempDir::new().unwrap();
    let bin = TempDir::new().unwrap();
    let path = tool_path(bin.path(), false);

    let output = run_hook(&repo_path(PLUGIN_SESSION_START), home.path(), &path, "{}");

    assert_eq!(output.status.code(), Some(0));
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|err| {
            panic!(
                "SessionStart must print exactly one JSON object ({err}): {}",
                String::from_utf8_lossy(&output.stdout)
            )
        });
    let context = stdout["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(context.contains("Aegis is not installed"), "{context}");
    assert!(context.contains("every Bash command is blocked"), "{context}");
    assert!(context.contains("npm i -g @iliasalmerekov/aegis"), "{context}");
}

#[test]
fn installed_session_start_resolves_the_rendered_binary_path() {
    let home = TempDir::new().unwrap();
    install_claude_settings_hooks(home.path());
    let bin = TempDir::new().unwrap();
    let path = tool_path(bin.path(), false);

    let output = run_hook(
        &home.path().join(".claude/hooks/aegis-session-start.sh"),
        home.path(),
        &path,
        "{}",
    );

    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let context = stdout["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(
        context.starts_with("IMPORTANT: All Bash tool commands must be routed through aegis."),
        "the installed hook carries an absolute binary path and must not report a missing binary: {context}"
    );
}
