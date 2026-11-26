use aegis_explanation::PolicyRationale;
use aegis_types::{AnalysisStatus, AnalysisSummary, Assessment, DegradationReason, RiskLevel};

use super::{make_assessment, make_explanation, make_language_match, strip_ansi};
use crate::shared::PromptDecision;
use crate::stdout_renderer::show_confirmation_with_decision;

fn degraded_assessment(cmd: &str, reasons: Vec<DegradationReason>) -> Assessment {
    let mut assessment = make_assessment(cmd, RiskLevel::Safe, vec![]);
    assessment.analysis = Some(AnalysisSummary {
        status: AnalysisStatus::Degraded,
        degradation_reasons: reasons,
    });
    assessment
}

fn render_text(
    assessment: &Assessment,
    rationale: PolicyRationale,
    is_interactive: bool,
    input: &[u8],
) -> (PromptDecision, String) {
    let explanation = make_explanation(assessment, rationale, None, None);
    let mut output = Vec::new();
    let mut reader = input;
    let decision = show_confirmation_with_decision(
        assessment,
        &explanation,
        &[],
        is_interactive,
        &mut reader,
        &mut output,
    );
    (decision, strip_ansi(&String::from_utf8_lossy(&output)))
}

#[test]
fn noninteractive_protect_degradation_uses_neutral_header_and_reason_phrase() {
    for (cmd, reason) in [
        ("$X -rf /", DegradationReason::DynamicSource),
        ("bash ./missing.sh", DegradationReason::UnsafeSource),
    ] {
        let assessment = degraded_assessment(cmd, vec![reason]);
        let (decision, text) = render_text(
            &assessment,
            PolicyRationale::AnalysisConfirmationRequired,
            false,
            b"y\n",
        );

        assert_eq!(decision, PromptDecision::Deny);
        assert!(
            text.contains("non-interactive mode — one-time approval required"),
            "missing neutral header for {cmd}: {text}"
        );
        let expected_reason = format!(
            "Reason: Aegis could not resolve what this command runs: {}",
            reason.phrase()
        );
        assert!(
            text.contains(&expected_reason),
            "missing {expected_reason:?} for {cmd}: {text}"
        );
        assert!(
            !text.to_ascii_lowercase().contains("language"),
            "output for {cmd} must not mention language: {text}"
        );
        assert!(
            text.contains("Hint: rerun interactively to make an interactive one-time decision.")
        );
        assert!(text.contains("Hint: this decision cannot be persisted or reused."));
        assert!(
            text.contains("Hint: rerun with --output json for machine-readable policy details.")
        );
    }
}

#[test]
fn noninteractive_strict_degradation_uses_neutral_header_and_reason_phrase() {
    let assessment = degraded_assessment("$X -rf /", vec![DegradationReason::DynamicSource]);
    let (decision, text) = render_text(
        &assessment,
        PolicyRationale::AnalysisOverrideRequired,
        false,
        b"y\n",
    );

    assert_eq!(decision, PromptDecision::Deny);
    assert!(
        text.contains("non-interactive mode — one-time approval required"),
        "{text}"
    );
    assert!(
        text.contains(
            "Reason: Aegis could not resolve what this command runs: a command word, argument, or working directory is only known at run time"
        ),
        "{text}"
    );
    assert!(
        !text.to_ascii_lowercase().contains("language"),
        "Strict denial must not mention language: {text}"
    );
}

#[test]
fn duplicate_degradation_reason_prints_once_and_distinct_reasons_print_in_order() {
    let dup = degraded_assessment(
        "$A $B",
        vec![
            DegradationReason::DynamicSource,
            DegradationReason::DynamicSource,
        ],
    );
    let (_, dup_text) = render_text(
        &dup,
        PolicyRationale::AnalysisConfirmationRequired,
        false,
        b"n\n",
    );
    assert_eq!(dup_text.matches("  Reason: ").count(), 1, "{dup_text}");

    let two = degraded_assessment(
        "$A ./missing.sh",
        vec![
            DegradationReason::DynamicSource,
            DegradationReason::UnsafeSource,
            DegradationReason::DynamicSource,
        ],
    );
    let (_, two_text) = render_text(
        &two,
        PolicyRationale::AnalysisConfirmationRequired,
        false,
        b"n\n",
    );
    assert_eq!(two_text.matches("  Reason: ").count(), 2, "{two_text}");
    let first = two_text
        .find(DegradationReason::DynamicSource.phrase())
        .expect("DynamicSource phrase present");
    let second = two_text
        .find(DegradationReason::UnsafeSource.phrase())
        .expect("UnsafeSource phrase present");
    assert!(
        first < second,
        "reasons must keep first-seen order:\n{two_text}"
    );
}

#[test]
fn interactive_protect_degradation_shows_reason_phrase_no_debug_names_and_neutral_prompt() {
    let assessment = degraded_assessment("$X -rf /", vec![DegradationReason::DynamicSource]);
    let (decision, text) = render_text(
        &assessment,
        PolicyRationale::AnalysisConfirmationRequired,
        true,
        b"n\n",
    );

    assert_eq!(decision, PromptDecision::Deny);
    assert!(
        text.contains(
            "Reason: Aegis could not resolve what this command runs: a command word, argument, or working directory is only known at run time"
        ),
        "{text}"
    );
    assert!(
        !text.to_ascii_lowercase().contains("language"),
        "interactive Protect dialog must not mention language: {text}"
    );
    assert!(
        !text.contains("DynamicSource") && !text.contains("Degraded"),
        "interactive dialog must not leak Rust Debug names: {text}"
    );
    assert!(
        text.contains("Approve this command once? [y/N]: "),
        "missing Protect prompt in:\n{text}"
    );
}

#[test]
fn interactive_strict_degradation_shows_reason_phrase_and_keeps_override_prompt() {
    let assessment =
        degraded_assessment("bash ./missing.sh", vec![DegradationReason::UnsafeSource]);
    let (decision, text) = render_text(
        &assessment,
        PolicyRationale::AnalysisOverrideRequired,
        true,
        b"n\n",
    );

    assert_eq!(decision, PromptDecision::Deny);
    assert!(
        text.contains(
            "Reason: Aegis could not resolve what this command runs: a source file is missing, unreadable, or unsafe to read"
        ),
        "{text}"
    );
    assert!(
        !text.to_ascii_lowercase().contains("language"),
        "interactive Strict dialog must not mention language: {text}"
    );
    assert!(
        !text.contains("UnsafeSource") && !text.contains("Degraded"),
        "interactive Strict dialog must not leak Rust Debug names: {text}"
    );
    assert!(
        text.contains("Approve one-time analysis override? [y/N]: "),
        "Strict prompt must stay unchanged:\n{text}"
    );
}

#[test]
fn language_match_prints_rule_id_and_detected_operation_without_source_text() {
    let matched = make_language_match("LANG-FS-DEL", RiskLevel::Warn, "TOP_SECRET_SOURCE_BODY");
    let assessment = make_assessment("python3 danger.py", RiskLevel::Warn, vec![matched]);

    for is_interactive in [false, true] {
        let (_, text) = render_text(
            &assessment,
            PolicyRationale::AnalysisConfirmationRequired,
            is_interactive,
            b"n\n",
        );
        assert!(
            text.contains(
                "Reason: matched rule LANG-FS-DEL: Filesystem deletion detected in source"
            ),
            "missing Match reason line (interactive={is_interactive}):\n{text}"
        );
        assert!(
            !text.contains("TOP_SECRET_SOURCE_BODY"),
            "source bytes must never appear in reason output (interactive={is_interactive}):\n{text}"
        );
    }
}

#[test]
fn mixed_language_match_and_degradation_prints_both_reason_lines() {
    let matched = make_language_match("LANG-FS-DEL", RiskLevel::Warn, "TOP_SECRET_SOURCE_BODY");
    let mut assessment = make_assessment("python3 danger.py", RiskLevel::Warn, vec![matched]);
    assessment.analysis = Some(AnalysisSummary {
        status: AnalysisStatus::Degraded,
        degradation_reasons: vec![DegradationReason::IncompleteSyntax],
    });

    let (_, text) = render_text(
        &assessment,
        PolicyRationale::AnalysisConfirmationRequired,
        false,
        b"n\n",
    );
    assert!(
        text.contains("Reason: matched rule LANG-FS-DEL: Filesystem deletion detected in source"),
        "{text}"
    );
    assert!(
        text.contains(
            "Reason: Aegis could not resolve what this command runs: a script could not be fully parsed"
        ),
        "{text}"
    );
    assert!(!text.contains("TOP_SECRET_SOURCE_BODY"), "{text}");
}

#[test]
fn analysis_denial_with_no_reasons_falls_back_without_language_wording() {
    for (rationale, expected) in [
        (
            PolicyRationale::AnalysisConfirmationRequired,
            "Reason: analysis requires one-time confirmation",
        ),
        (
            PolicyRationale::AnalysisOverrideRequired,
            "Reason: analysis override required",
        ),
    ] {
        let assessment = make_assessment("sh ./run.sh", RiskLevel::Safe, vec![]);
        let (_, text) = render_text(&assessment, rationale, false, b"n\n");
        assert!(text.contains(expected), "missing {expected:?} in:\n{text}");
        assert!(
            !text.to_ascii_lowercase().contains("language"),
            "fallback denial must not mention language:\n{text}"
        );
    }
}
