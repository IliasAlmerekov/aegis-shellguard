use super::*;

// ── Recovery opt-out: no persistent approval (ADR-031 section 4) ──────────

fn opted_out_danger() -> (Assessment, CommandExplanation) {
    let p = make_match(
        "FS-001",
        RiskLevel::Danger,
        r"rm\s+",
        "Recursive delete",
        None,
    );
    let assessment = make_assessment("rm -rf /home/user", RiskLevel::Danger, vec![p]);
    let mut explanation = default_explanation_for_assessment(&assessment);
    explanation.context.recovery_opt_out = true;
    (assessment, explanation)
}

#[test]
fn recovery_opt_out_danger_prompt_hides_always_and_says_why() {
    let (assessment, explanation) = opted_out_danger();

    let mut output = Vec::new();
    crate::stdout_renderer::show_confirmation_with_decision(
        &assessment,
        &explanation,
        &[],
        true,
        &mut b"n\n".as_ref(),
        &mut output,
    );

    let text = strip_ansi(&String::from_utf8_lossy(&output));
    assert!(
        text.contains("Persistent approval is unavailable while Snapshot recovery is disabled."),
        "got:\n{text}"
    );
    assert!(text.contains("[y/N/d]:"), "got:\n{text}");
    assert!(!text.contains("/a/"), "got:\n{text}");
}

#[test]
fn recovery_opt_out_danger_still_approves_once() {
    let (assessment, explanation) = opted_out_danger();

    let decision = crate::stdout_renderer::show_confirmation_with_decision(
        &assessment,
        &explanation,
        &[],
        true,
        &mut b"y\n".as_ref(),
        &mut Vec::new(),
    );

    assert_eq!(decision, PromptDecision::Approve);
}

#[test]
fn recovery_opt_out_danger_rejects_a_typed_always() {
    let (assessment, explanation) = opted_out_danger();

    for typed in ["a\n", "always\n"] {
        let decision = crate::stdout_renderer::show_confirmation_with_decision(
            &assessment,
            &explanation,
            &[],
            true,
            &mut typed.as_bytes(),
            &mut Vec::new(),
        );
        assert_eq!(decision, PromptDecision::Deny, "typed {typed:?}");
    }
}
