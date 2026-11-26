#![cfg(feature = "language-analysis")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail loudly"
)]

mod support;

use std::time::Duration;

use aegis::analysis::{AnalysisCwd, OrchestrationBudget, Outcome, run_with_budget_in_cwd};
use aegis_types::RiskLevel;
use support::safe_baseline;

fn dangerous_recursive_delete() -> &'static str {
    "import shutil\nshutil.rmtree('x')\n"
}

fn shell_grammar_workspace() -> tempfile::TempDir {
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

#[tokio::test]
async fn run_finds_a_delete_chained_on_a_heredoc_marker_line() {
    let workspace = shell_grammar_workspace();
    assert_recursive_delete_found_in(&workspace, "cat <<A && python3 ./evil.py\nhi\nA").await;
}

#[tokio::test]
async fn run_finds_a_delete_behind_a_leading_output_redirection() {
    let workspace = shell_grammar_workspace();
    assert_recursive_delete_found_in(&workspace, ">out python3 ./evil.py").await;
}

#[tokio::test]
async fn run_finds_a_delete_fed_in_through_stdin_redirection() {
    let workspace = shell_grammar_workspace();
    assert_recursive_delete_found_in(&workspace, "python3 < ./evil.py").await;
}

#[tokio::test]
async fn run_finds_a_delete_inside_a_function_body_at_definition() {
    let workspace = shell_grammar_workspace();
    assert_recursive_delete_found_in(&workspace, "f(){ python3 ./evil.py; }; f").await;
}
