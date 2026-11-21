use super::*;
// ── Block ─────────────────────────────────────────────────────────────────

#[test]
fn block_always_returns_false() {
    let p = make_match(
        "PS-006",
        RiskLevel::Block,
        "rm",
        "Deletes root filesystem",
        None,
    );
    let assessment = make_assessment("rm -rf /", RiskLevel::Block, vec![p]);

    // Even if the user somehow types "yes", Block must return false.
    let result = show_confirmation_with_input(
        &assessment,
        &default_explanation_for_assessment(&assessment),
        &[],
        true,
        &mut b"yes\n".as_ref(),
        &mut Vec::new(),
    );
    assert!(!result, "Block must always return false");
}

#[test]
fn block_output_contains_reason() {
    let p = make_match(
        "PS-006",
        RiskLevel::Block,
        "rm",
        "Kills the root filesystem",
        None,
    );
    let assessment = make_assessment("rm -rf /", RiskLevel::Block, vec![p]);

    let mut output = Vec::new();
    show_confirmation_with_input(
        &assessment,
        &default_explanation_for_assessment(&assessment),
        &[],
        true,
        &mut b"".as_ref(),
        &mut output,
    );

    let text = strip_ansi(&String::from_utf8_lossy(&output));
    assert!(
        text.contains("Kills the root filesystem"),
        "Block output must contain the pattern description; got:\n{text}"
    );
}

#[test]
fn block_output_contains_command() {
    let p = make_match("PS-006", RiskLevel::Block, "rm", "Root delete", None);
    let assessment = make_assessment("rm -rf /", RiskLevel::Block, vec![p]);

    let mut output = Vec::new();
    show_confirmation_with_input(
        &assessment,
        &default_explanation_for_assessment(&assessment),
        &[],
        true,
        &mut b"".as_ref(),
        &mut output,
    );

    let text = strip_ansi(&String::from_utf8_lossy(&output));
    assert!(
        text.contains("rm -rf /"),
        "Block output must contain the command; got:\n{text}"
    );
}

#[test]
fn block_and_noninteractive_denials_still_deny_when_output_fails() {
    use std::io::{self, Write};

    use crate::stdout_renderer::show_confirmation_with_decision;

    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("write failed"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("flush failed"))
        }
    }

    let block = make_assessment(
        "rm -rf /",
        RiskLevel::Block,
        vec![make_match(
            "PS-006",
            RiskLevel::Block,
            "rm",
            "Root delete",
            None,
        )],
    );
    let block_exp = default_explanation_for_assessment(&block);
    assert_eq!(
        show_confirmation_with_decision(
            &block,
            &block_exp,
            &[],
            true,
            &mut b"yes\n".as_ref(),
            &mut FailingWriter,
        ),
        PromptDecision::Deny,
    );
    assert!(!show_confirmation_with_input(
        &block,
        &block_exp,
        &[],
        true,
        &mut b"yes\n".as_ref(),
        &mut FailingWriter,
    ));

    for risk in [RiskLevel::Danger, RiskLevel::Warn] {
        let assessment = make_assessment(
            "rm -rf build",
            risk,
            vec![make_match("FS-001", risk, "rm", "Recursive delete", None)],
        );
        let explanation = default_explanation_for_assessment(&assessment);
        assert_eq!(
            show_confirmation_with_decision(
                &assessment,
                &explanation,
                &[],
                false,
                &mut b"yes\n".as_ref(),
                &mut FailingWriter,
            ),
            PromptDecision::Deny,
        );
    }

    for rationale in [
        PolicyRationale::AnalysisConfirmationRequired,
        PolicyRationale::AnalysisOverrideRequired,
    ] {
        let assessment = make_assessment(
            "python danger.py",
            RiskLevel::Danger,
            vec![make_language_match("LA-001", RiskLevel::Danger, "unlink")],
        );
        let explanation = make_explanation(&assessment, rationale, None, None);
        assert_eq!(
            show_confirmation_with_decision(
                &assessment,
                &explanation,
                &[],
                false,
                &mut b"yes\n".as_ref(),
                &mut FailingWriter,
            ),
            PromptDecision::Deny,
        );
    }

    let policy_exp = make_explanation(
        &block,
        PolicyRationale::StrictPolicy,
        Some(BlockReason::StrictPolicy),
        None,
    );
    render_policy_block(&block, &policy_exp, &mut FailingWriter);
}
