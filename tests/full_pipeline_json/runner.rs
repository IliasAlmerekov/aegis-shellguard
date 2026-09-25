//! Runner routing (#421, ADR-040) at the CLI JSON seam. The CLI clamps the
//! language-analysis deadline to 100 ms, so under load a worker can miss it and
//! the command falls back to a prompt with no language Match (#458). These
//! tests only assert what holds either way: exit code 2, `decision = prompt`,
//! and no `auto_approve`. Matches, risk, and degradation are asserted in
//! `tests/analysis_orchestrate/runner.rs`, which passes an explicit deadline.

use super::*;

const DANGER_PY: &str = "import shutil\nshutil.rmtree('artifact')\n";

fn assert_prompts(home: &TempDir, cwd: &std::path::Path, command: &str) {
    let output = base_command(home.path())
        .current_dir(cwd)
        .args(["-c", command, "--output", "json"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2), "{command}");
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["decision"], "prompt", "{command}: {json}");
}

#[test]
fn uv_run_harmless_inline_python_uses_exec_002() {
    // EXEC-002 is a scanner Match, so it survives an analysis fallback.
    let home = TempDir::new().unwrap();
    let output = base_command(home.path())
        .args(["-c", "uv run python -c 'print(1)'", "--output", "json"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["risk"], "warn", "{json}");
    assert_eq!(json["decision"], "prompt", "{json}");
    assert!(
        json["matched_patterns"]
            .as_array()
            .is_some_and(|matches| matches.iter().any(|item| item["id"] == "EXEC-002")),
        "{json}"
    );
}

#[test]
fn runner_python_script_forms_prompt() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    fs::write(workspace.path().join("danger.py"), DANGER_PY).unwrap();
    fs::write(workspace.path().join("danger"), DANGER_PY).unwrap();
    fs::write(workspace.path().join("python3"), DANGER_PY).unwrap();

    for command in [
        "uv run python3 danger.py",
        "uv run ./danger.py",
        "/usr/bin/uv run ./danger.py",
        "uv run --script ./danger",
        "uv run --script ./python3",
        "uvx python3 danger.py",
        "uv tool run python3 danger.py",
        "/usr/bin/uv tool run python3 danger.py",
        "pipx run python3 danger.py",
        "pipx run ./danger.py",
        "/usr/bin/pipx run ./danger.py",
        "poetry run python3 danger.py",
        "pipenv run python3 danger.py",
    ] {
        assert_prompts(&home, workspace.path(), command);
    }
}

#[test]
fn npx_explicit_node_script_prompts() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    fs::write(
        workspace.path().join("danger.js"),
        "const fs = require('node:fs');\nfs.rmSync('artifact', { recursive: true });\n",
    )
    .unwrap();

    assert_prompts(&home, workspace.path(), "npx node danger.js");
}

#[test]
fn uv_directory_option_with_bare_relative_script_prompts() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    fs::create_dir(workspace.path().join("elsewhere")).unwrap();
    fs::write(
        workspace.path().join("elsewhere").join("danger.py"),
        DANGER_PY,
    )
    .unwrap();

    for command in [
        "uv run --directory ./elsewhere ./danger.py",
        "uv --directory ./elsewhere run ./danger.py",
    ] {
        assert_prompts(&home, workspace.path(), command);
    }
}

#[test]
fn reported_runner_inline_source_prompts() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let python = "python3 -c \"import shutil; shutil.rmtree('artifact')\"";
    let node = "node -e \"fs.rmSync('artifact', {recursive: true})\"";
    for command in [
        format!("uv run {python}"),
        format!("uvx {python}"),
        format!("poetry run {python}"),
        format!("pipenv run {python}"),
        format!("pipx run {python}"),
        format!("npx {node}"),
        format!("uv tool run {python}"),
    ] {
        assert_prompts(&home, workspace.path(), &command);
    }
}

#[test]
fn direct_exec_operand_behind_package_selecting_runner_prompts() {
    // `npx`, `uvx`, `pipx run`, and `uv tool run` (the long form of `uvx`)
    // pick their own child binary; a harmless direct-exec operand cannot
    // rule out a different package-provided executable actually running
    // (#421, ADR-040).
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    write_executable(&workspace.path().join("clean-bin"), "#!/bin/sh\necho hi\n");
    write_executable(
        &workspace.path().join("danger-bin"),
        "#!/usr/bin/env python3\nimport shutil\nshutil.rmtree('artifact')\n",
    );

    for runner in ["npx", "uvx", "pipx run", "uv tool run"] {
        for operand in ["./clean-bin", "./danger-bin"] {
            assert_prompts(&home, workspace.path(), &format!("{runner} {operand}"));
        }
    }
}

#[test]
fn runner_subcommands_without_a_program_stay_auto_approved() {
    // These start no language analysis (see
    // `runner_subcommands_without_a_program_start_no_analysis`), so no worker
    // deadline can turn them into a fallback prompt.
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();

    for command in ["uv sync", "poetry install"] {
        let output = base_command(home.path())
            .current_dir(workspace.path())
            .args(["-c", command, "--output", "json"])
            .output()
            .unwrap();

        assert_eq!(output.status.code(), Some(0), "{command}");
        let json: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(json["decision"], "auto_approve", "{command}: {json}");
    }
}
