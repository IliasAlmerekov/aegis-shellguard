#![cfg(feature = "language-analysis")]

mod support;

use std::time::Duration;

use aegis::analysis::{AnalysisCwd, OrchestrationBudget, Outcome, run_with_budget_in_cwd};
use aegis_types::{AnalysisStatus, RiskLevel};
use support::safe_baseline;

fn dangerous_recursive_delete() -> &'static str {
    "import shutil\nshutil.rmtree('x')\n"
}

fn benign_decoy() -> &'static str {
    "print('ok')\n"
}

fn wrapper_cd_workspace() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("temp workspace");
    std::fs::create_dir_all(workspace.path().join("d1/sub")).expect("mkdir d1/sub");
    std::fs::create_dir_all(workspace.path().join("sub")).expect("mkdir sub");
    std::fs::write(
        workspace.path().join("d1/sub/evil.py"),
        dangerous_recursive_delete(),
    )
    .expect("write d1/sub/evil.py");
    std::fs::write(workspace.path().join("sub/evil.py"), benign_decoy())
        .expect("write sub/evil.py");
    workspace
}

#[tokio::test]
async fn run_joins_a_literal_cd_inside_a_subshell_and_reads_the_right_file() {
    let workspace = wrapper_cd_workspace();
    let baseline = safe_baseline();
    let outcome = run_with_budget_in_cwd(
        "(cd -- d1 && python3 ./sub/evil.py)",
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
        other => panic!("a literal cd inside a subshell must still spawn the worker: {other:?}"),
    };
    assert!(
        assessment.risk >= RiskLevel::Danger,
        "must read d1/sub/evil.py (dangerous), not sub/evil.py (benign): {:?}",
        assessment.risk
    );
    assert!(
        assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "LANG-FS-DEL-R"),
        "must carry a LANG-FS-DEL-R match from d1/sub/evil.py: {:?}",
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref().to_string())
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn run_degrades_rather_than_auto_approving_a_non_literal_cd_inside_a_subshell() {
    let workspace = wrapper_cd_workspace();
    let baseline = safe_baseline();
    let outcome = run_with_budget_in_cwd(
        "(cd d1 && python3 ./sub/evil.py)",
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
        other => panic!("a cd-tainted subshell must never silently report no analysis: {other:?}"),
    };
    assert_eq!(
        assessment.analysis.as_ref().map(|analysis| analysis.status),
        Some(AnalysisStatus::Degraded),
        "{assessment:?}"
    );
}
