use super::*;

use std::path::Path;

use aegis::analysis::{AnalysisCwd, run_with_budget_in_cwd};
use aegis_scanner::{PatternSet, Scanner};

async fn run_in(command: &str, cwd: &Path, scanner: Option<&Scanner>) -> Assessment {
    let outcome = run_with_budget_in_cwd(
        command,
        AnalysisCwd::Resolved(cwd),
        &safe_baseline(),
        Some(env!("CARGO_BIN_EXE_aegis")),
        &[],
        OrchestrationBudget {
            total_timeout: Duration::from_secs(5),
            ..OrchestrationBudget::L1_DEFAULT
        },
        scanner,
    )
    .await;
    match outcome {
        Outcome::Analyzed { assessment, .. } => assessment,
        other => panic!("{command}: must be analyzed: {other:?}"),
    }
}

fn has_match(assessment: &Assessment, id: &str) -> bool {
    assessment
        .matched
        .iter()
        .any(|item| item.pattern.id.as_ref() == id)
}

fn builtin_scanner() -> Scanner {
    Scanner::try_new(PatternSet::load().expect("built-in patterns load"))
        .expect("built-in scanner compiles")
}

#[tokio::test]
async fn inline_os_remove_completes_with_a_warn_lang_fs_del_match() {
    let outcome = run_with_budget(
        "python3 -c 'import os; os.remove(\"artifact.txt\")'",
        &safe_baseline(),
        Some(env!("CARGO_BIN_EXE_aegis")),
        &[],
        budget_with_timeout(Duration::from_secs(5)),
    )
    .await;
    let Outcome::Analyzed { assessment, .. } = outcome else {
        panic!("inline os.remove must be analyzed: {outcome:?}");
    };
    assert!(has_match(&assessment, "LANG-FS-DEL"), "{assessment:?}");
    assert_eq!(assessment.risk, RiskLevel::Warn);
    let summary = assessment.analysis.as_ref().expect("analysis must be set");
    assert_eq!(summary.status, AnalysisStatus::Complete, "{summary:?}");
}

#[tokio::test]
async fn trusted_alias_reaches_routing_and_yields_lang_fs_del() {
    let outcome = run_with_budget(
        "trusted-python -c 'import os; os.remove(\"artifact.txt\")'",
        &safe_baseline(),
        Some(env!("CARGO_BIN_EXE_aegis")),
        &[("trusted-python", "python3")],
        budget_with_timeout(Duration::from_secs(5)),
    )
    .await;
    let Outcome::Analyzed { assessment, .. } = outcome else {
        panic!("aliased inline source must be analyzed: {outcome:?}");
    };
    assert!(has_match(&assessment, "LANG-FS-DEL"), "{assessment:?}");
}

#[tokio::test]
async fn relative_interpreter_script_resolves_against_the_given_cwd() {
    let workspace = tempfile::tempdir().expect("temp workspace");
    std::fs::write(
        workspace.path().join("run.py"),
        "import os\nos.remove('artifact.txt')\n",
    )
    .unwrap();

    let assessment = run_in("python3 ./run.py", workspace.path(), None).await;
    assert!(has_match(&assessment, "LANG-FS-DEL"), "{assessment:?}");
    let summary = assessment.analysis.as_ref().expect("analysis must be set");
    assert_eq!(summary.status, AnalysisStatus::Complete, "{summary:?}");
}

#[tokio::test]
async fn safe_shell_script_completes_without_a_match() {
    let scanner = builtin_scanner();
    let workspace = tempfile::tempdir().expect("temp workspace");
    write_executable(&workspace.path().join("probe.sh"), "#!/bin/sh\necho 1\n");

    for command in ["./probe.sh", "sh ./probe.sh"] {
        let assessment = run_in(command, workspace.path(), Some(&scanner)).await;
        assert!(assessment.matched.is_empty(), "{command}: {assessment:?}");
        let summary = assessment.analysis.as_ref().expect("analysis must be set");
        assert_ne!(
            summary.status,
            AnalysisStatus::Degraded,
            "{command}: {summary:?}"
        );
    }
}

#[tokio::test]
async fn shell_script_hiding_a_force_push_yields_git_003_without_its_source() {
    let scanner = builtin_scanner();
    let workspace = tempfile::tempdir().expect("temp workspace");
    write_executable(
        &workspace.path().join("deploy.sh"),
        "#!/bin/sh\necho deploying\ngit push --force origin main\n",
    );

    let assessment = run_in("./deploy.sh", workspace.path(), Some(&scanner)).await;
    let git_003 = assessment
        .matched
        .iter()
        .find(|item| item.pattern.id.as_ref() == "GIT-003")
        .unwrap_or_else(|| panic!("must carry a GIT-003 Match: {assessment:?}"));
    assert!(
        !git_003.matched_text.contains("origin main"),
        "the Match must not disclose script source"
    );
}
