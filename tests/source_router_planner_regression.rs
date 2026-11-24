//! Full-planner regression tests for source-router coverage (ADR-054, issue #212).
//!
//! Drives the `aegis` binary through `--output json` (which runs `plan_with_context`)
//! across every advisory class handled by the grammar-free source router, plus the
//! scanner-level GHSA-7564 control. Must pass on both default and `language-analysis`
//! feature builds.

mod support;

use std::fs;
use std::path::Path;

use serde_json::Value;
use tempfile::TempDir;

use support::{base_command, write_executable};

const DANGER_PY: &str = "import shutil\nshutil.rmtree('artifact')\n";
const DANGER_SHEBANG_PY: &str =
    "#!/usr/bin/env python3\nimport shutil\nshutil.rmtree('artifact')\n";

fn evaluate_json(home: &Path, cwd: &Path, command: &str) -> Value {
    let output = base_command(home)
        .current_dir(cwd)
        .args(["-c", command, "--output", "json"])
        .output()
        .unwrap();

    assert!(
        output.stderr.is_empty(),
        "stderr must stay empty in json mode for `{command}`: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "invalid json for `{command}`: {err}\nstdout: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert_eq!(json["execution"]["will_execute"], false, "{command}");
    json
}

fn assert_requires_approval(home: &Path, cwd: &Path, command: &str) {
    let json = evaluate_json(home, cwd, command);
    assert_eq!(
        json["decision"], "prompt",
        "DynamicSource / control shape must require approval for `{command}`: {json}"
    );
}

fn assert_script_target_never_auto_approves_without_snapshot(
    home: &Path,
    cwd: &Path,
    command: &str,
) {
    let json = evaluate_json(home, cwd, command);
    let decision = json["decision"]
        .as_str()
        .unwrap_or_else(|| panic!("missing decision for `{command}`: {json}"));
    let snapshot_requested = json["snapshot_plan"]["requested"]
        .as_bool()
        .unwrap_or_else(|| panic!("missing snapshot_plan.requested for `{command}`: {json}"));

    assert!(
        matches!(decision, "prompt" | "block")
            || (decision == "auto_approve" && snapshot_requested),
        "script target must never auto-approve without a required snapshot for `{command}`: {json}"
    );
}

#[test]
fn later_list_segment_script_targets_never_auto_approve_without_snapshot() {
    // GHSA-c5pv
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    fs::write(workspace.path().join("script.py"), DANGER_PY).unwrap();

    for command in [
        "true; python3 script.py",
        "true && python3 script.py",
        "false || python3 script.py",
    ] {
        assert_script_target_never_auto_approves_without_snapshot(
            home.path(),
            workspace.path(),
            command,
        );
    }
}

#[test]
fn command_runner_script_targets_never_auto_approve_without_snapshot() {
    // GHSA-399c / ADR-040
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    fs::write(workspace.path().join("script.py"), DANGER_PY).unwrap();

    for command in [
        "uv run python3 script.py",
        "uv run ./script.py",
        "uvx python3 script.py",
    ] {
        assert_script_target_never_auto_approves_without_snapshot(
            home.path(),
            workspace.path(),
            command,
        );
    }
}

#[test]
fn executor_values_and_dynamic_launcher_operands_require_approval() {
    // GHSA-xj54
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    fs::write(workspace.path().join("script.py"), DANGER_PY).unwrap();

    for command in [
        "git -c diff.external='python3 ./script.py' diff",
        "setsid python3 ./script.py",
        "setsid $HOME/pyx",
    ] {
        assert_requires_approval(home.path(), workspace.path(), command);
    }
}

#[test]
fn resolved_launcher_operand_script_targets_never_auto_approve_without_snapshot() {
    // GHSA-xj54
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    write_executable(&workspace.path().join("shebang-script"), DANGER_SHEBANG_PY);
    write_executable(&home.path().join("pyx"), DANGER_SHEBANG_PY);

    for command in ["setsid ./shebang-script", "setsid ~/pyx"] {
        assert_script_target_never_auto_approves_without_snapshot(
            home.path(),
            workspace.path(),
            command,
        );
    }
}

#[test]
fn ansi_c_quoted_unclaimed_stages_require_approval() {
    // GHSA-xprr
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();

    for command in [
        r#"strace -o $'\'$(echo x' ./pyx ')'"#,
        r#"sed $'s/\t/ /' file"#,
        r#"strace -o$'\'' ./pyx #'"#,
    ] {
        assert_requires_approval(home.path(), workspace.path(), command);
    }
}

#[test]
fn dynamic_shell_words_require_approval() {
    // ADR-043
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();

    for command in [
        "$X -rf /",
        "cat /dev/null >&$F",
        "declare -n r=EDITOR; r=reboot; $EDITOR",
    ] {
        assert_requires_approval(home.path(), workspace.path(), command);
    }
}

#[test]
fn git_global_options_control_still_requires_approval() {
    // GHSA-7564
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();

    for command in [
        "git -C . reset --hard HEAD~1",
        "git --no-pager reset --hard",
        "git -c k=v reset --hard",
    ] {
        assert_requires_approval(home.path(), workspace.path(), command);
    }
}
