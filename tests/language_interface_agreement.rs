//! Supported-interface agreement regression for Language-aware analysis.

mod support;

use std::env;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use serde_json::Value;
use tempfile::TempDir;

const ANALYZED_COMMAND: &str = "python3 -c 'import os; os.remove(\"fixture\")'";

#[derive(Debug, PartialEq, Eq)]
struct CanonicalOutcome {
    assessment: Value,
    decision: String,
}

/// `ci_policy = "Allow"` neutralizes the CI-transport-only fail-closed policy
/// (default `Block`, which would make the CI arm exit non-`Denied` for reasons
/// unrelated to Language-aware analysis and break agreement with Shell/Watch/
/// hooks). CI's default fail-closed posture is covered separately by
/// `analysis_orchestrate_runtime.rs`; this suite is only about whether every
/// transport reaches the same Assessment+Decision once that policy is neutral.
fn configure_ci_allow(home: &Path) {
    support::write_global_config(
        home,
        "ci_policy = \"Allow\"\n[language_analysis]\ntimeout_ms = 1000\n",
    );
}

fn aegis(home: &Path, cwd: &Path, ci: bool) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_aegis"));
    command
        .env("HOME", home)
        .env("AEGIS_REAL_SHELL", "/bin/sh")
        .env("AEGIS_FORCE_NO_TTY", "1")
        .env("AEGIS_CI", if ci { "1" } else { "0" })
        .current_dir(cwd);
    command
}

fn audit_outcome(home: &Path) -> CanonicalOutcome {
    let entries = support::read_audit_entries(home);
    let value = entries
        .first()
        .expect("each fixture home audits exactly one command");
    assert_eq!(
        entries.len(),
        1,
        "each fixture home must audit exactly one command, found {}",
        entries.len()
    );
    CanonicalOutcome {
        // Audit is the common typed public Assessment projection for every
        // supported transport. Keep every behavior-bearing field rather than
        // reducing agreement to risk and pattern IDs; timestamps and the raw
        // command are intentionally transport-independent metadata, not part
        // of the Assessment+Decision contract under test.
        assessment: serde_json::json!({
            "risk": value["risk"],
            "matched_patterns": value["matched_patterns"],
            "pattern_ids": value["pattern_ids"],
            "basis": value["basis"],
            "analysis": value["analysis"],
            "effect_opaque": value["effect_opaque"],
        }),
        decision: value["decision"]
            .as_str()
            .expect("audit decision")
            .to_string(),
    }
}

fn invoke_hook(home: &Path, script: &str) -> Output {
    let input = serde_json::json!({ "tool_input": { "command": ANALYZED_COMMAND } }).to_string();
    let mut command = Command::new("/bin/sh");
    command
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("scripts")
                .join(script),
        )
        .env("HOME", home)
        .env("AEGIS_BIN", env!("CARGO_BIN_EXE_aegis"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("hook fixture must spawn");
    child
        .stdin
        .as_mut()
        .expect("hook stdin")
        .write_all(input.as_bytes())
        .expect("hook input");
    child.wait_with_output().expect("hook fixture must finish")
}

#[cfg(unix)]
fn run_forwarded_hook_command(home: &Path, cwd: &Path, hook_output: &Output) {
    use std::os::unix::fs::symlink;

    let hook: Value = serde_json::from_slice(&hook_output.stdout).expect("hook response JSON");
    let forwarded = hook["hookSpecificOutput"]["updatedInput"]["command"]
        .as_str()
        .expect("hook must forward a wrapped command");
    let bin_dir = tempfile::tempdir().expect("temporary PATH directory");
    symlink(env!("CARGO_BIN_EXE_aegis"), bin_dir.path().join("aegis")).expect("Aegis PATH symlink");
    let inherited_path = env::var("PATH").expect("PATH is set for integration tests");
    let status = Command::new("/bin/sh")
        .args(["-c", forwarded])
        .env("HOME", home)
        .env("AEGIS_REAL_SHELL", "/bin/sh")
        .env("AEGIS_FORCE_NO_TTY", "1")
        .env("AEGIS_CI", "0")
        .env(
            "PATH",
            format!("{}:{inherited_path}", bin_dir.path().display()),
        )
        .current_dir(cwd)
        .status()
        .expect("forwarded hook command must run");
    assert_eq!(
        status.code(),
        Some(2),
        "hooked analysis must be denied without a TTY"
    );
}

#[cfg(unix)]
#[test]
fn language_analysis_agrees_across_shell_watch_hooks_and_ci() {
    let cwd = TempDir::new().expect("temporary command cwd");

    let shell_home = TempDir::new().expect("temporary Shell home");
    configure_ci_allow(shell_home.path());
    let shell = aegis(shell_home.path(), cwd.path(), false)
        .args(["--command", ANALYZED_COMMAND])
        .output()
        .expect("Shell evaluation");
    assert_eq!(shell.status.code(), Some(2));
    let expected = audit_outcome(shell_home.path());

    let ci_home = TempDir::new().expect("temporary CI home");
    configure_ci_allow(ci_home.path());
    let ci = aegis(ci_home.path(), cwd.path(), true)
        .args(["--command", ANALYZED_COMMAND])
        .output()
        .expect("CI evaluation");
    assert_eq!(ci.status.code(), Some(2));
    assert_eq!(audit_outcome(ci_home.path()), expected);

    let watch_home = TempDir::new().expect("temporary Watch home");
    configure_ci_allow(watch_home.path());
    let watch_input = format!(r#"{{"cmd":{ANALYZED_COMMAND:?},"id":"agreement"}}"#) + "\n";
    let watch = aegis(watch_home.path(), cwd.path(), false)
        .arg("watch")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Watch must spawn");
    // `Child` owns its stdin, so use a scoped mutable binding to send the sole frame.
    let mut watch = watch;
    watch
        .stdin
        .as_mut()
        .expect("Watch stdin")
        .write_all(watch_input.as_bytes())
        .expect("Watch input");
    let watch = watch.wait_with_output().expect("Watch must finish");
    assert!(
        watch.status.success(),
        "Watch owns a long-running transport exit code"
    );
    let watch_result: Value = String::from_utf8_lossy(&watch.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("Watch NDJSON frame"))
        .find(|frame: &Value| frame["type"] == "result")
        .expect("Watch result frame");
    assert_eq!(watch_result["decision"], "denied");
    assert_eq!(watch_result["exit_code"], 2);
    assert_eq!(audit_outcome(watch_home.path()), expected);

    for script in ["hooks/claude-code.sh", "hooks/codex-pre-tool-use.sh"] {
        let hook_home = TempDir::new().expect("temporary hook home");
        configure_ci_allow(hook_home.path());
        let hook = invoke_hook(hook_home.path(), script);
        assert!(
            hook.status.success(),
            "{script} must return a hook response"
        );
        run_forwarded_hook_command(hook_home.path(), cwd.path(), &hook);
        assert_eq!(audit_outcome(hook_home.path()), expected, "{script}");
    }
}
