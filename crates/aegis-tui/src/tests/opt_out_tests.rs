use super::*;

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

fn opted_out_warn() -> (Assessment, CommandExplanation) {
    let p = make_match("GIT-009", RiskLevel::Warn, r"git\s+", "Opaque", None);
    let assessment = make_assessment("git reset", RiskLevel::Warn, vec![p]);
    let mut explanation = default_explanation_for_assessment(&assessment);
    explanation.context.recovery_opt_out = true;
    (assessment, explanation)
}

#[test]
fn recovery_opt_out_warn_prompt_hides_always_and_says_why() {
    let (assessment, explanation) = opted_out_warn();

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
}

#[test]
fn recovery_opt_out_warn_rejects_a_typed_always() {
    let (assessment, explanation) = opted_out_warn();

    let decision = crate::stdout_renderer::show_confirmation_with_decision(
        &assessment,
        &explanation,
        &[],
        true,
        &mut b"a\n".as_ref(),
        &mut Vec::new(),
    );
    assert_eq!(decision, PromptDecision::Deny);
}

#[test]
fn ordinary_warn_prompt_still_offers_always() {
    let (assessment, mut explanation) = opted_out_warn();
    explanation.context.recovery_opt_out = false;

    let mut output = Vec::new();
    let decision = crate::stdout_renderer::show_confirmation_with_decision(
        &assessment,
        &explanation,
        &[],
        true,
        &mut b"a\n".as_ref(),
        &mut output,
    );

    let text = strip_ansi(&String::from_utf8_lossy(&output));
    assert!(text.contains("[y/N/a/d]:"), "got:\n{text}");
    assert_eq!(decision, PromptDecision::ApproveAlways);
}

#[test]
fn no_snapshot_danger_requires_full_yes() {
    let (assessment, mut explanation) = opted_out_danger();
    explanation.context.recovery_opt_out = false;
    explanation.context.no_rollback_disclosure =
        Some(aegis_explanation::NO_ROLLBACK_DISCLOSURE.to_string());
    let mut output = Vec::new();
    let decision = crate::stdout_renderer::show_confirmation_with_decision(
        &assessment,
        &explanation,
        &[],
        true,
        &mut b"y\n".as_ref(),
        &mut output,
    );
    assert_eq!(decision, PromptDecision::Deny);
    let text = strip_ansi(&String::from_utf8_lossy(&output));
    assert!(text.contains("No Snapshot provider applies: this command cannot be rolled back."));
    assert!(text.contains("Type the full word yes to run a command that cannot be rolled back."));
}

#[test]
fn no_snapshot_prompt_choices_and_red_disclosure() {
    let (assessment, mut explanation) = opted_out_danger();
    explanation.context.recovery_opt_out = false;
    explanation.context.no_rollback_disclosure =
        Some(aegis_explanation::NO_ROLLBACK_DISCLOSURE.to_string());
    for (answer, expected) in [
        ("yes\n", PromptDecision::Approve),
        (" YES \n", PromptDecision::Approve),
        ("y\n", PromptDecision::Deny),
        ("a\n", PromptDecision::Deny),
        ("always\n", PromptDecision::Deny),
        ("denyalways\n", PromptDecision::Deny),
        ("deny-always\n", PromptDecision::Deny),
        ("\n", PromptDecision::Deny),
        ("d\n", PromptDecision::DenyAlways),
    ] {
        let mut output = Vec::new();
        let result = crate::stdout_renderer::show_confirmation_with_decision(
            &assessment,
            &explanation,
            &[],
            true,
            &mut answer.as_bytes(),
            &mut output,
        );
        assert_eq!(result, expected, "{answer:?}");
        let raw = String::from_utf8(output).unwrap();
        if std::env::var_os("NO_COLOR").is_none() {
            assert!(
                raw.contains("\x1b[38;5;9m\n  No Snapshot provider applies:"),
                "{raw:?}"
            );
        }
        let text = strip_ansi(&raw);
        assert!(text.contains("[yes/N/d]"));
        assert!(!text.contains("/a/"));
        if expected == PromptDecision::Deny {
            assert!(
                text.contains(
                    "Type the full word yes to run a command that cannot be rolled back."
                )
            );
        }
    }
}

#[test]
fn no_snapshot_noninteractive_discloses_before_denial() {
    let (assessment, mut explanation) = opted_out_danger();
    explanation.context.recovery_opt_out = false;
    explanation.context.no_rollback_disclosure =
        Some(aegis_explanation::NO_ROLLBACK_DISCLOSURE.to_string());
    let mut output = Vec::new();
    let result = crate::stdout_renderer::show_confirmation_with_decision(
        &assessment,
        &explanation,
        &[],
        false,
        &mut b"".as_ref(),
        &mut output,
    );
    assert_eq!(result, PromptDecision::Deny);
    assert!(
        String::from_utf8(output)
            .unwrap()
            .starts_with(aegis_explanation::NO_ROLLBACK_DISCLOSURE)
    );
}

#[test]
fn applicable_provider_with_unresolved_git_target_still_accepts_y() {
    let (assessment, mut explanation) = opted_out_danger();
    explanation.context.recovery_opt_out = false;
    explanation
        .context
        .applicable_snapshot_plugins
        .push("docker".into());
    explanation
        .context
        .git_targets
        .push(aegis_explanation::GitTargetExplanation::Unresolved {
            reason: "dynamic path".into(),
        });
    let mut output = Vec::new();
    let decision = crate::stdout_renderer::show_confirmation_with_decision(
        &assessment,
        &explanation,
        &[],
        true,
        &mut b"y\n".as_ref(),
        &mut output,
    );
    assert_eq!(decision, PromptDecision::Approve);
    let text = strip_ansi(&String::from_utf8_lossy(&output));
    assert!(text.contains("Git target unresolved"));
    assert!(!text.contains("No Snapshot provider applies"));
}

#[test]
fn applicable_provider_without_unresolved_target_still_accepts_y() {
    let (assessment, mut explanation) = opted_out_danger();
    explanation.context.recovery_opt_out = false;
    explanation
        .context
        .applicable_snapshot_plugins
        .push("git".into());
    let mut output = Vec::new();
    let decision = crate::stdout_renderer::show_confirmation_with_decision(
        &assessment,
        &explanation,
        &[],
        true,
        &mut b"y\n".as_ref(),
        &mut output,
    );
    assert_eq!(decision, PromptDecision::Approve);
    let text = strip_ansi(&String::from_utf8_lossy(&output));
    assert!(!text.contains("No Snapshot provider applies"));
}
