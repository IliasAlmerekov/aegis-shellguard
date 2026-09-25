//! Analysis-seam coverage for CLI-level claims narrowed by issue #458.
//!
//! `language_analysis.timeout_ms` clamps to 100ms at every config layer
//! (`LANGUAGE_ANALYSIS_TIMEOUT_MS`, ADR-022 §6/§7) — a CLI-spawned `aegis`
//! process can never be given a bigger budget from a test, so under a loaded
//! `cargo test --workspace` run the worker can miss that deadline and degrade
//! instead of completing. Several CLI-level tests dropped or loosened an
//! assertion that only holds once analysis Completes, pointing here for the
//! part they can no longer prove. These tests call the same
//! `aegis::analysis` orchestration through a multi-second budget instead of
//! the clamped config path, so the dropped claims stay covered.

use std::path::Path;
use std::time::Duration;

use aegis::analysis::{AnalysisCwd, OrchestrationBudget, Outcome, run, run_with_budget_in_cwd};
use aegis_scanner::{PatternSet, Scanner};
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

#[cfg(unix)]
fn write_executable(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, body).expect("fixture script must be written");
    let mut permissions = std::fs::metadata(path)
        .expect("fixture script metadata")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).expect("fixture script must be made executable");
}

fn builtin_scanner() -> Scanner {
    Scanner::try_new(PatternSet::load().expect("built-in patterns load"))
        .expect("built-in scanner compiles")
}

/// Keeps the claim `tests/full_pipeline_json.rs`'s
/// `json_output_uses_language_aware_assessment_before_policy` and
/// `tests/watch_mode.rs`'s
/// `watch_without_tty_denies_language_aware_match_before_execution` both
/// narrowed away (#458): the identical inline `os.remove` command really
/// does complete with a Warn LANG-FS-DEL Match, given a budget the CLI
/// cannot obtain.
#[tokio::test]
async fn inline_os_remove_yields_lang_fs_del_warn_given_a_real_budget() {
    let baseline = safe_baseline();
    let outcome = run(
        "python3 -c 'import os; os.remove(\"artifact.txt\")'",
        &baseline,
        Some(env!("CARGO_BIN_EXE_aegis")),
        &[],
        Duration::from_secs(5),
    )
    .await;
    let assessment = match outcome {
        Outcome::Analyzed { assessment, .. } => assessment,
        other => panic!("inline os.remove must spawn the worker: {other:?}"),
    };
    assert_eq!(assessment.risk, RiskLevel::Warn, "{assessment:?}");
    assert!(
        assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "LANG-FS-DEL"),
        "must carry a LANG-FS-DEL match: {assessment:?}"
    );
    let summary = assessment.analysis.as_ref().expect("analysis must be set");
    assert_eq!(summary.status, AnalysisStatus::Complete, "{summary:?}");
}

/// Keeps the claim `tests/full_pipeline_json.rs`'s
/// `trusted_global_alias_reaches_language_aware_routing` narrowed away
/// (#458): the `trusted_aliases` parameter genuinely reaches `route`, and
/// the aliased command still yields the LANG-FS-DEL Match.
#[tokio::test]
async fn trusted_alias_forwards_into_routing_and_yields_lang_fs_del() {
    let baseline = safe_baseline();
    let outcome = run(
        "trusted-python -c 'import os; os.remove(\"artifact.txt\")'",
        &baseline,
        Some(env!("CARGO_BIN_EXE_aegis")),
        &[("trusted-python", "python3")],
        Duration::from_secs(5),
    )
    .await;
    let assessment = match outcome {
        Outcome::Analyzed { assessment, .. } => assessment,
        other => panic!("aliased inline command must spawn the worker: {other:?}"),
    };
    assert!(
        assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "LANG-FS-DEL"),
        "the trusted alias must reach routing: {assessment:?}"
    );
}

/// Keeps the claim `tests/watch_mode.rs`'s
/// `watch_resolves_relative_script_file_against_frame_cwd` narrowed away
/// (#458). A missing-file degradation also denies, so `analysis.status ==
/// "complete"` was the only CLI-visible proof that the frame's `cwd` — not
/// the process cwd — resolved `./run.py`. `AnalysisCwd::Resolved` is the
/// same mechanism Watch's frame-cwd threading feeds into, so a Complete
/// LANG-FS-DEL result here means a real `run.py` at that directory was read.
#[tokio::test]
async fn relative_interpreter_script_resolves_against_explicit_cwd() {
    let workspace = tempfile::tempdir().expect("temp workspace");
    std::fs::write(
        workspace.path().join("run.py"),
        "import os\nos.remove('artifact.txt')\n",
    )
    .expect("write run.py");
    let baseline = safe_baseline();

    let outcome = run_with_budget_in_cwd(
        "python3 ./run.py",
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
        other => panic!("relative script must be analyzed: {other:?}"),
    };
    assert!(
        assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "LANG-FS-DEL"),
        "must resolve run.py against the given cwd and find its Match: {assessment:?}"
    );
    let summary = assessment.analysis.as_ref().expect("analysis must be set");
    assert_eq!(summary.status, AnalysisStatus::Complete, "{summary:?}");
}

/// Keeps the claim `tests/language_bash_pipeline.rs`'s
/// `directly_executed_safe_shell_script_is_auto_approved` and
/// `shell_script_run_through_sh_is_auto_approved_when_safe` narrowed away
/// (#458): both invocation shapes of a genuinely safe script complete with
/// no Match, given a real budget.
#[tokio::test]
async fn safe_shell_script_completes_with_no_match_in_both_invocation_shapes() {
    let scanner = builtin_scanner();
    for command in ["./probe.sh", "sh ./probe.sh"] {
        let workspace = tempfile::tempdir().expect("temp workspace");
        write_executable(&workspace.path().join("probe.sh"), "#!/bin/sh\necho 1\n");
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
            Some(&scanner),
        )
        .await;
        let assessment = match outcome {
            Outcome::Analyzed { assessment, .. } => assessment,
            other => panic!("{command}: safe script must be analyzed: {other:?}"),
        };
        assert!(
            assessment.matched.is_empty(),
            "{command}: a safe script must produce no Match: {assessment:?}"
        );
        let summary = assessment.analysis.as_ref().expect("analysis must be set");
        assert_ne!(
            summary.status,
            AnalysisStatus::Degraded,
            "{command}: a safe script must not degrade given a real budget: {summary:?}"
        );
    }
}

/// Keeps the claim `tests/language_bash_pipeline.rs`'s
/// `shell_script_hiding_a_risky_command_still_prompts` narrowed away
/// (#458): a script file hiding `git push --force` really does surface the
/// same GIT-003 Match it gets when typed, without disclosing the script
/// source (ADR-022 §10).
#[tokio::test]
async fn shell_script_hiding_a_force_push_yields_git_003_without_disclosing_source() {
    let scanner = builtin_scanner();
    let workspace = tempfile::tempdir().expect("temp workspace");
    write_executable(
        &workspace.path().join("deploy.sh"),
        "#!/bin/sh\necho deploying\ngit push --force origin main\n",
    );
    let baseline = safe_baseline();

    let outcome = run_with_budget_in_cwd(
        "./deploy.sh",
        AnalysisCwd::Resolved(workspace.path()),
        &baseline,
        Some(env!("CARGO_BIN_EXE_aegis")),
        &[],
        OrchestrationBudget {
            total_timeout: Duration::from_secs(5),
            ..OrchestrationBudget::L1_DEFAULT
        },
        Some(&scanner),
    )
    .await;
    let assessment = match outcome {
        Outcome::Analyzed { assessment, .. } => assessment,
        other => panic!("script hiding a force push must be analyzed: {other:?}"),
    };
    let git_003 = assessment
        .matched
        .iter()
        .find(|m| m.pattern.id.as_ref() == "GIT-003")
        .unwrap_or_else(|| panic!("must carry a GIT-003 match: {assessment:?}"));
    assert!(
        !git_003.matched_text.contains("origin main"),
        "the Match must not disclose script source"
    );
}
