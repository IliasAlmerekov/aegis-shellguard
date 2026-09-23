//! Decision-level regression tests for the #384/#430 round-2 findings (H1
//! heredoc-chain regression, G1 shell-grammar gaps). Real-subprocess seam,
//! same as `tests/analysis_orchestrate.rs` and its `wrapper_cd` sibling:
//! these exercise the full route -> spawn -> Analyze -> map -> merge
//! composition through `aegis::analysis::run_with_budget_in_cwd`.

use std::time::Duration;

use aegis::analysis::{AnalysisCwd, OrchestrationBudget, Outcome, run_with_budget_in_cwd};
use aegis_types::{Assessment, ParsedCommand, RiskLevel};

fn safe_baseline() -> Assessment {
    Assessment {
        risk: RiskLevel::Safe,
        effect_opaque: false,
        matched: Vec::new(),
        highlight_ranges: Vec::new(),
        command: ParsedCommand {
            program: None,
            argv: Vec::new(),
            normalized: String::new(),
            inline_scripts: Vec::new(),
            raw: String::new(),
        },
        analysis: None,
    }
}

fn dangerous_recursive_delete() -> &'static str {
    "import shutil\nshutil.rmtree('x')\n"
}

fn round2_workspace() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("temp workspace");
    std::fs::write(
        workspace.path().join("evil.py"),
        dangerous_recursive_delete(),
    )
    .expect("write evil.py");
    workspace
}

async fn assert_recursive_delete_found_in(workspace: &tempfile::TempDir, command: &str) {
    let baseline = safe_baseline();
    let outcome = run_with_budget_in_cwd(
        command,
        AnalysisCwd::Resolved(workspace.path()),
        &baseline,
        Some(env!("CARGO_BIN_EXE_aegis")),
        &[],
        OrchestrationBudget {
            total_timeout: Duration::from_secs(5),
            ..OrchestrationBudget::L1_DEFAULT
        },
        None,
    )
    .await;
    let assessment = match outcome {
        Outcome::Analyzed { assessment, .. } => assessment,
        other => panic!("{command} must spawn the worker: {other:?}"),
    };
    assert!(
        assessment.risk >= RiskLevel::Danger,
        "{command}: risk must lift to Danger: {:?}",
        assessment.risk
    );
    assert!(
        assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "LANG-FS-DEL-R"),
        "{command}: must carry a LANG-FS-DEL-R match: {:?}",
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref().to_string())
            .collect::<Vec<_>>()
    );
}

// ── H1: a command chained on a heredoc marker's own line used to vanish ────

#[tokio::test]
async fn run_finds_a_delete_chained_on_a_heredoc_marker_line() {
    let workspace = round2_workspace();
    assert_recursive_delete_found_in(&workspace, "cat <<A && python3 ./evil.py\nhi\nA").await;
}

// ── G1/L1: leading redirection, stdin redirection, and a function body ─────

#[tokio::test]
async fn run_finds_a_delete_behind_a_leading_output_redirection() {
    let workspace = round2_workspace();
    assert_recursive_delete_found_in(&workspace, ">out python3 ./evil.py").await;
}

#[tokio::test]
async fn run_finds_a_delete_fed_in_through_stdin_redirection() {
    let workspace = round2_workspace();
    assert_recursive_delete_found_in(&workspace, "python3 < ./evil.py").await;
}

#[tokio::test]
async fn run_finds_a_delete_inside_a_function_body_at_definition() {
    let workspace = round2_workspace();
    assert_recursive_delete_found_in(&workspace, "f(){ python3 ./evil.py; }; f").await;
}
