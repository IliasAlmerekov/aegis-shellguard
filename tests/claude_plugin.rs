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
    for tool in ["bash", "tr", "printenv", "cat", "grep"] {
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
        let plugin_out = run_hook(
            &repo_path(PLUGIN_PRE_TOOL_USE),
            home.path(),
            &path,
            &payload,
        );
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
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "SessionStart must print exactly one JSON object ({err}): {}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    let context = stdout["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(context.contains("Aegis is not installed"), "{context}");
    assert!(
        context.contains("every Bash command is blocked"),
        "{context}"
    );
    assert!(
        context.contains("npm i -g @iliasalmerekov/aegis"),
        "{context}"
    );
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

fn read_repo_json(relative: &str) -> serde_json::Value {
    serde_json::from_str(&read_repo_file(relative))
        .unwrap_or_else(|err| panic!("{relative} is not valid JSON: {err}"))
}

/// The single command a hooks.json event registers under `matcher`.
fn registered_command<'a>(hooks: &'a serde_json::Value, event: &str, matcher: &str) -> &'a str {
    let entries = hooks["hooks"][event].as_array().unwrap();
    assert_eq!(entries.len(), 1, "{event} must have exactly one entry");
    assert_eq!(entries[0]["matcher"], matcher);
    let commands = entries[0]["hooks"].as_array().unwrap();
    assert_eq!(commands.len(), 1, "{event} must run exactly one command");
    assert_eq!(commands[0]["type"], "command");
    commands[0]["command"].as_str().unwrap()
}

#[test]
fn claude_plugin_manifests_are_consistent() {
    let hooks = read_repo_json("plugins/aegis/hooks/hooks.json");
    assert_eq!(
        registered_command(&hooks, "PreToolUse", "Bash"),
        "\"${CLAUDE_PLUGIN_ROOT}/hooks/aegis-pre-tool-use.sh\""
    );
    assert_eq!(
        registered_command(&hooks, "SessionStart", "startup|resume"),
        "\"${CLAUDE_PLUGIN_ROOT}/hooks/aegis-session-start.sh\""
    );

    let plugin = read_repo_json("plugins/aegis/.claude-plugin/plugin.json");
    assert_eq!(plugin["name"], "aegis");
    assert_eq!(plugin["version"], env!("CARGO_PKG_VERSION"));

    let marketplace = read_repo_json(".claude-plugin/marketplace.json");
    assert_eq!(marketplace["name"], "aegis-shellguard");
    assert!(marketplace["owner"]["name"].is_string());
    let plugins = marketplace["plugins"].as_array().unwrap();
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0]["name"], "aegis");
    assert_eq!(plugins[0]["source"], "./plugins/aegis");
}

#[test]
fn claude_plugin_rollback_command_runs_through_bash_tool() {
    let command = read_repo_file("plugins/aegis/commands/aegis-rollback.md");
    let front_matter = command
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map(|(front, _)| front)
        .expect("aegis-rollback.md must open with YAML front matter");
    assert!(
        front_matter
            .lines()
            .any(|line| line.trim() == "disable-model-invocation: true"),
        "front matter must set disable-model-invocation: true:\n{front_matter}"
    );
    // `!`-prefixed lines run in Claude Code before the prompt reaches the
    // model, outside the Bash tool, so PreToolUse and Aegis never see them.
    assert!(
        !command.contains("!`"),
        "the command must not inject shell output"
    );
    for required in ["aegis snapshot list", "confirm", "aegis rollback"] {
        assert!(command.contains(required), "missing `{required}`");
    }
}

/// Where `status_reports_claude_hook_registration` puts the aegis-managed
/// PreToolUse entry.
#[derive(Clone, Copy, Debug)]
enum EntryScope {
    Absent,
    User,
    ProjectLocal,
}

/// A settings object with an aegis-managed PreToolUse Bash entry.
fn settings_with_aegis_entry(dir: &Path) -> serde_json::Value {
    let shim = dir.join(".claude/hooks/aegis-pre-tool-use.sh");
    serde_json::json!({
        "hooks": {
            "PreToolUse": [{
                "matcher": "Bash",
                "hooks": [{ "type": "command", "command": shim.display().to_string() }]
            }]
        }
    })
}

#[test]
fn status_reports_claude_hook_registration() {
    for (plugin_enabled, entry, expected) in [
        (true, EntryScope::Absent, "claude code hooks: plugin"),
        (false, EntryScope::User, "claude code hooks: settings"),
        (
            false,
            EntryScope::ProjectLocal,
            "claude code hooks: settings",
        ),
        (false, EntryScope::Absent, "claude code hooks: none"),
        (true, EntryScope::User, "claude code hooks: duplicate"),
        (
            true,
            EntryScope::ProjectLocal,
            "claude code hooks: duplicate",
        ),
    ] {
        let home = TempDir::new().unwrap();
        let project = TempDir::new().unwrap();
        // The status line prints the path from the canonical cwd, which on
        // macOS is /private/var/... for a /var/folders/... temp dir.
        let project_dir = fs::canonicalize(project.path()).unwrap();
        let user_settings = home.path().join(".claude/settings.json");
        let local_settings = project_dir.join(".claude/settings.local.json");
        let mut user_json = match entry {
            EntryScope::User => settings_with_aegis_entry(home.path()),
            _ => serde_json::json!({}),
        };
        user_json["enabledPlugins"] =
            serde_json::json!({ "aegis@aegis-shellguard": plugin_enabled });
        fs::create_dir_all(user_settings.parent().unwrap()).unwrap();
        fs::write(&user_settings, user_json.to_string()).unwrap();
        if let EntryScope::ProjectLocal = entry {
            fs::create_dir_all(local_settings.parent().unwrap()).unwrap();
            fs::write(
                &local_settings,
                settings_with_aegis_entry(&project_dir).to_string(),
            )
            .unwrap();
        }

        let output = Command::new(env!("CARGO_BIN_EXE_aegis"))
            .arg("status")
            .env("HOME", home.path())
            .current_dir(&project_dir)
            .output()
            .unwrap();

        assert_eq!(output.status.code(), Some(0));
        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout
            .lines()
            .find(|line| line.starts_with("claude code hooks: "))
            .unwrap_or_else(|| panic!("no claude code hooks line:\n{stdout}"));
        assert!(line.starts_with(expected), "{entry:?}: {line}");
        if expected.ends_with("duplicate") {
            let settings = match entry {
                EntryScope::ProjectLocal => &local_settings,
                _ => &user_settings,
            };
            assert!(line.contains(&settings.display().to_string()), "{line}");
            assert!(line.contains("runs twice per command"), "{line}");
            assert!(line.contains("by hand"), "{line}");
        } else {
            assert_eq!(line, expected, "{entry:?}");
        }
    }
}

#[test]
fn release_script_bumps_the_claude_code_plugin_version() {
    // The script downloads release checksums, so it cannot run offline; pin
    // that it rewrites the plugin manifest next to package.json.
    let script = read_repo_file("scripts/update-npm-package.sh");
    assert!(
        script.contains("plugins/aegis/.claude-plugin/plugin.json"),
        "the release script must bump the Claude Code plugin version with the npm version"
    );
}

/// Copy `plugins/aegis/hooks/` into `<root>/hooks/`, keeping the modes.
fn copy_plugin_hooks(root: &Path) {
    let source = repo_path("plugins/aegis/hooks");
    fs::create_dir_all(root.join("hooks")).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), root.join("hooks").join(entry.file_name())).unwrap();
    }
}

#[test]
fn claude_plugin_hook_commands_run_from_a_root_with_spaces() {
    // Claude Code runs each hooks.json command through a shell with
    // CLAUDE_PLUGIN_ROOT set. An unquoted root with a space splits into two
    // words, the hook cannot start, and Claude Code lets Bash run unguarded.
    let base = TempDir::new().unwrap();
    let root = base.path().join("plugin cache").join("aegis");
    copy_plugin_hooks(&root);
    let home = TempDir::new().unwrap();
    let bin = TempDir::new().unwrap();
    let path = tool_path(bin.path(), true);
    let hooks = read_repo_json("plugins/aegis/hooks/hooks.json");

    for (event, matcher, stdin) in [
        ("PreToolUse", "Bash", bash_payload("ls -la")),
        ("SessionStart", "startup|resume", "{}".to_string()),
    ] {
        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg(registered_command(&hooks, event, matcher))
            .env_clear()
            .env("HOME", home.path())
            .env("PATH", &path)
            .env("CLAUDE_PLUGIN_ROOT", &root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();

        assert_eq!(
            output.status.code(),
            Some(0),
            "{event}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(stdout["hookSpecificOutput"]["hookEventName"], event);
    }
}
