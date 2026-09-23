//! Slice B — #384 R1: a `cd` inside a wrapper body is no longer ignored.
//! Split from `tests/analysis_orchestrate.rs` to stay under the repo's
//! 800-line file budget; real-subprocess seam, same as its sibling.
//!
//! A cd hidden inside a `(...)`/`{...}`/reserved-word wrapper used to be
//! invisible to routing: the whole wrapped stage either dropped out of
//! routing entirely (if it happened to be a whole-stage `(...)`/`{...}` wrap)
//! or the cd was silently skipped while a relative target elsewhere in the
//! same body kept resolving against the *outer* cwd. Both let a command read
//! the wrong file while still reporting Safe. `d1/sub/evil.py` is the
//! dangerous payload; `sub/evil.py` is a decoy with the same tail.

use std::time::Duration;

use aegis::analysis::{AnalysisCwd, OrchestrationBudget, Outcome, run_with_budget_in_cwd};
use aegis_types::{AnalysisStatus, Assessment, ParsedCommand, RiskLevel};

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
    // `cd d1` (no `--`) is not the router's trusted literal shape, so the
    // subshell's own cwd walk degrades rather than resolving `./sub/evil.py`
    // against a directory it cannot prove — never `NotStarted` (the pre-fix
    // behavior, which silently reported Safe with no analysis at all).
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
