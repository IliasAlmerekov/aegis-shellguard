//! Runner routing (#421, ADR-040) at the analysis seam. Matches, risk, and
//! degradation live here rather than in `tests/full_pipeline_json/runner.rs`:
//! the CLI cannot raise the 100 ms analysis deadline, so a loaded machine can
//! turn any CLI-level Match into a fallback prompt (#458). These tests pass an
//! explicit 5 s budget instead.

use super::*;

use std::path::Path;

use aegis::analysis::{AnalysisCwd, OrchestrationBudget, run_with_budget_in_cwd};

const DANGER_PY: &str = "import shutil\nshutil.rmtree('artifact')\n";
const DANGER_JS: &str =
    "const fs = require('node:fs');\nfs.rmSync('artifact', { recursive: true });\n";

async fn run_in(command: &str, cwd: &Path) -> Outcome {
    run_with_budget_in_cwd(
        command,
        AnalysisCwd::Resolved(cwd),
        &safe_baseline(),
        Some(env!("CARGO_BIN_EXE_aegis")),
        &[],
        OrchestrationBudget {
            total_timeout: Duration::from_secs(5),
            ..OrchestrationBudget::L1_DEFAULT
        },
        None,
    )
    .await
}

fn analyzed(command: &str, outcome: Outcome) -> Assessment {
    match outcome {
        Outcome::Analyzed { assessment, .. } => assessment,
        other => panic!("{command}: runner source must be analyzed: {other:?}"),
    }
}

fn has_recursive_delete(assessment: &Assessment) -> bool {
    assessment
        .matched
        .iter()
        .any(|item| item.pattern.id.as_ref() == "LANG-FS-DEL-R")
}

fn write_executable(path: &Path, body: &str) {
    std::fs::write(path, body).expect("write executable");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).unwrap();
    }
}

/// Asserts a `LANG-FS-DEL-R` Match lifted to Danger with complete analysis.
async fn assert_complete_recursive_delete(command: &str, cwd: &Path) {
    let assessment = analyzed(command, run_in(command, cwd).await);
    assert!(
        has_recursive_delete(&assessment),
        "{command}: {assessment:?}"
    );
    assert_eq!(assessment.risk, RiskLevel::Danger, "{command}");
    let summary = assessment.analysis.as_ref().expect("analysis must be set");
    assert_eq!(summary.status, AnalysisStatus::Complete, "{command}");
}

/// Asserts a `LANG-FS-DEL-R` Match lifted to Danger that keeps `DynamicSource`
/// degradation, because the runner may select a different executable.
async fn assert_recursive_delete_with_dynamic_source(command: &str, cwd: &Path) {
    let assessment = analyzed(command, run_in(command, cwd).await);
    assert!(
        has_recursive_delete(&assessment),
        "{command}: {assessment:?}"
    );
    assert_eq!(assessment.risk, RiskLevel::Danger, "{command}");
    let summary = assessment.analysis.as_ref().expect("analysis must be set");
    assert_eq!(summary.status, AnalysisStatus::Degraded, "{command}");
    assert!(
        summary
            .degradation_reasons
            .contains(&DegradationReason::DynamicSource),
        "{command}: {summary:?}"
    );
}

/// Asserts no Match and `DynamicSource` degradation.
async fn assert_degraded_without_match(command: &str, cwd: &Path) {
    let assessment = analyzed(command, run_in(command, cwd).await);
    assert!(assessment.matched.is_empty(), "{command}: {assessment:?}");
    let summary = assessment.analysis.as_ref().expect("analysis must be set");
    assert_eq!(summary.status, AnalysisStatus::Degraded, "{command}");
    assert!(
        summary
            .degradation_reasons
            .contains(&DegradationReason::DynamicSource),
        "{command}: {summary:?}"
    );
}

#[tokio::test]
async fn runner_explicit_interpreter_script_uses_language_match() {
    let workspace = tempfile::tempdir().expect("temp workspace");
    std::fs::write(workspace.path().join("danger.py"), DANGER_PY).unwrap();

    for command in [
        "uv run python3 danger.py",
        "poetry run python3 danger.py",
        "pipenv run python3 danger.py",
    ] {
        assert_complete_recursive_delete(command, workspace.path()).await;
    }
}

#[tokio::test]
async fn package_selecting_runner_explicit_interpreter_script_keeps_match_and_degradation() {
    // `uvx`, `uv tool run` (the long form of `uvx`), `pipx run`, and `npx`
    // may install a package that provides its own `python3` or `node`.
    let workspace = tempfile::tempdir().expect("temp workspace");
    std::fs::write(workspace.path().join("danger.py"), DANGER_PY).unwrap();
    std::fs::write(workspace.path().join("danger.js"), DANGER_JS).unwrap();

    for command in [
        "uvx python3 danger.py",
        "uv tool run python3 danger.py",
        "/usr/bin/uv tool run python3 danger.py",
        "pipx run python3 danger.py",
        "npx node danger.js",
    ] {
        assert_recursive_delete_with_dynamic_source(command, workspace.path()).await;
    }
}

#[tokio::test]
async fn runner_bare_python_script_uses_language_match() {
    let workspace = tempfile::tempdir().expect("temp workspace");
    std::fs::write(workspace.path().join("danger.py"), DANGER_PY).unwrap();

    for command in [
        "uv run ./danger.py",
        "/usr/bin/uv run ./danger.py",
        "pipx run ./danger.py",
        "/usr/bin/pipx run ./danger.py",
    ] {
        // A bare `.py` operand names its own source, so even `pipx run`
        // adds no `DynamicSource` degradation here (#421, ADR-040).
        assert_complete_recursive_delete(command, workspace.path()).await;
    }
}

#[tokio::test]
async fn uv_run_script_flag_routes_extensionless_and_interpreter_named_paths_as_python() {
    let workspace = tempfile::tempdir().expect("temp workspace");
    std::fs::write(workspace.path().join("danger"), DANGER_PY).unwrap();
    std::fs::write(workspace.path().join("python3"), DANGER_PY).unwrap();

    for command in ["uv run --script ./danger", "uv run --script ./python3"] {
        assert_complete_recursive_delete(command, workspace.path()).await;
    }
}

#[tokio::test]
async fn uv_directory_option_degrades_bare_relative_script() {
    let workspace = tempfile::tempdir().expect("temp workspace");
    std::fs::create_dir(workspace.path().join("elsewhere")).unwrap();
    std::fs::write(
        workspace.path().join("elsewhere").join("danger.py"),
        DANGER_PY,
    )
    .unwrap();

    for command in [
        "uv run --directory ./elsewhere ./danger.py",
        "uv --directory ./elsewhere run ./danger.py",
    ] {
        assert_degraded_without_match(command, workspace.path()).await;
    }
}

#[tokio::test]
async fn runner_inline_source_uses_language_match() {
    let workspace = tempfile::tempdir().expect("temp workspace");
    let python = "python3 -c \"import shutil; shutil.rmtree('artifact')\"";
    for command in [
        format!("uv run {python}"),
        format!("poetry run {python}"),
        format!("pipenv run {python}"),
    ] {
        assert_complete_recursive_delete(&command, workspace.path()).await;
    }
}

#[tokio::test]
async fn direct_exec_operand_behind_package_selecting_runner_degrades_clean_script() {
    // A harmless direct-exec operand cannot rule out a different
    // package-provided executable actually running (#421, ADR-040).
    let workspace = tempfile::tempdir().expect("temp workspace");
    write_executable(&workspace.path().join("clean-bin"), "#!/bin/sh\necho hi\n");

    for command in [
        "npx ./clean-bin",
        "uvx ./clean-bin",
        "pipx run ./clean-bin",
        "uv tool run ./clean-bin",
    ] {
        assert_degraded_without_match(command, workspace.path()).await;
    }
}

#[tokio::test]
async fn direct_exec_operand_controls_add_no_dynamic_source_degradation() {
    // A bare direct-exec operand and a runner that names its own child
    // program (`uv run`, `poetry run`) never select a package executable, so
    // routing must not add `DynamicSource` for them (#421, ADR-040). The
    // `#!/bin/sh` body itself degrades with `GrammarUnavailable`, which is
    // independent of runner routing.
    let workspace = tempfile::tempdir().expect("temp workspace");
    write_executable(&workspace.path().join("clean-bin"), "#!/bin/sh\necho hi\n");

    for command in [
        "./clean-bin",
        "uv run ./clean-bin",
        "poetry run ./clean-bin",
    ] {
        let assessment = analyzed(command, run_in(command, workspace.path()).await);
        assert!(assessment.matched.is_empty(), "{command}: {assessment:?}");
        assert_eq!(assessment.risk, RiskLevel::Safe, "{command}");
        let summary = assessment.analysis.as_ref().expect("analysis must be set");
        assert_eq!(
            summary.degradation_reasons,
            vec![DegradationReason::GrammarUnavailable],
            "{command}"
        );
    }
}

#[tokio::test]
async fn runner_subcommands_without_a_program_start_no_analysis() {
    let workspace = tempfile::tempdir().expect("temp workspace");
    for command in ["uv sync", "poetry install"] {
        match run_in(command, workspace.path()).await {
            Outcome::NotStarted { .. } => {}
            other => panic!("{command}: must not start a worker: {other:?}"),
        }
    }
}

#[tokio::test]
async fn direct_exec_operand_behind_package_selecting_runner_keeps_match_and_degradation() {
    // `npx`, `uvx`, `pipx run`, and `uv tool run` (the long form of `uvx`)
    // pick their own child binary; a visible-source Match on a direct-exec
    // operand never rules out a different package-provided executable
    // actually running (#421, ADR-040).
    let workspace = tempfile::tempdir().expect("temp workspace");
    write_executable(
        &workspace.path().join("danger-bin"),
        "#!/usr/bin/env python3\nimport shutil\nshutil.rmtree('artifact')\n",
    );

    for command in [
        "npx ./danger-bin",
        "uvx ./danger-bin",
        "pipx run ./danger-bin",
        "uv tool run ./danger-bin",
    ] {
        assert_recursive_delete_with_dynamic_source(command, workspace.path()).await;
    }
}

#[tokio::test]
async fn package_selected_interpreter_inline_source_keeps_match_and_degradation() {
    let workspace = tempfile::tempdir().expect("temp workspace");
    for command in [
        "npx node -e \"fs.rmSync('artifact', {recursive: true})\"",
        "/usr/bin/npx node -e \"fs.rmSync('artifact', {recursive: true})\"",
        "uvx python3 -c \"shutil.rmtree('artifact')\"",
        "/usr/bin/uvx python3 -c \"shutil.rmtree('artifact')\"",
        "pipx run python3 -c \"shutil.rmtree('artifact')\"",
        "/usr/bin/pipx run python3 -c \"shutil.rmtree('artifact')\"",
        "uv tool run python3 -c \"shutil.rmtree('artifact')\"",
        "/usr/bin/uv tool run python3 -c \"shutil.rmtree('artifact')\"",
    ] {
        assert_recursive_delete_with_dynamic_source(command, workspace.path()).await;
    }
}

#[tokio::test]
async fn opaque_runner_options_degrade_without_claiming_their_source() {
    // `npx -c` takes a command string, and `uv tool run --from` is an option
    // the parser can't see past. Neither may claim the trailing interpreter
    // as its source (#421, ADR-040).
    let workspace = tempfile::tempdir().expect("temp workspace");
    for command in [
        "npx -c \"node -e 'fs.rmSync(\\\"artifact\\\", {recursive: true})'\"",
        "/usr/bin/npx -c \"node -e 'fs.rmSync(\\\"artifact\\\", {recursive: true})'\"",
        "uv tool run --from x python3 -c \"shutil.rmtree('artifact')\"",
    ] {
        assert_degraded_without_match(command, workspace.path()).await;
    }
}
