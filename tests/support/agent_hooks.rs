//! Agent-hook shared helpers, extracted from `tests/agent_hooks.rs` so the
//! split agent-hook test crates (`agent_hooks`, `agent_hooks_install`) share
//! one set of script runners and JSON probes instead of each carrying a
//! verbatim copy. Same rationale as `support::installer`.
//!
//! These helpers are test-only and intentionally panic on internal errors
//! (`.unwrap()` is acceptable here).
#![expect(
    dead_code,
    reason = "each integration-test crate compiles this helper separately and uses a different subset"
)]

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;

/// CI detection variables the hooks consult. Cleared for every scripted run so
/// a developer's own environment cannot flip a hook into its CI-override
/// branch and mask a failure in the "outside CI" cases.
pub const CI_MARKER_VARS: [&str; 9] = [
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

pub fn script_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join(name)
}

pub fn aegis_test_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_aegis")
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("CARGO_BIN_EXE_aegis is not set for agent hook tests"))
}

pub fn prepare_agent_dirs(home: &Path, claude: bool, codex: bool) {
    if claude {
        fs::create_dir_all(home.join(".claude")).unwrap();
    }

    if codex {
        fs::create_dir_all(home.join(".codex")).unwrap();
    }
}

pub fn run_script(script_name: &str, home: &Path, args: &[&str], stdin: Option<&str>) -> Output {
    run_script_with_env(script_name, home, args, stdin, &[])
}

pub fn run_script_with_env(
    script_name: &str,
    home: &Path,
    args: &[&str],
    stdin: Option<&str>,
    envs: &[(&str, &str)],
) -> Output {
    let mut command = Command::new("/bin/sh");
    command.arg(script_path(script_name));
    command.args(args);
    command.env("HOME", home);
    command.env("AEGIS_BIN", aegis_test_binary());
    for key in CI_MARKER_VARS {
        command.env_remove(key);
    }
    // Applied after the removals so a case can deliberately reinstate a CI
    // marker to exercise the override branch.
    for (key, value) in envs {
        command.env(key, value);
    }
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command.spawn().unwrap();

    if let Some(input) = stdin {
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    }

    child.wait_with_output().unwrap()
}

pub fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

pub fn shell_quote(command: &str) -> String {
    format!("'{}'", command.replace('\'', r"'\''"))
}

/// True when `json["hooks"][section]` registers `command` on any entry.
pub fn json_contains_command(json: &Value, section: &str, command: &str) -> bool {
    json["hooks"][section].as_array().is_some_and(|entries| {
        entries.iter().any(|entry| {
            entry["hooks"]
                .as_array()
                .is_some_and(|hooks| hooks.iter().any(|hook| hook["command"] == command))
        })
    })
}

pub fn run_codex_pre_tool_use(home: &Path, command: &str) -> Output {
    let input = serde_json::json!({ "tool_input": { "command": command } }).to_string();
    run_script(
        "hooks/codex-pre-tool-use.sh",
        home,
        &[],
        Some(input.as_str()),
    )
}

pub fn run_claude_code_hook(home: &Path, command: &str) -> Output {
    let input = serde_json::json!({ "tool_input": { "command": command } }).to_string();
    run_script("hooks/claude-code.sh", home, &[], Some(input.as_str()))
}
