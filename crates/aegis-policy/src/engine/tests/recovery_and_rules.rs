use super::*;

fn evaluate_effect_opaque(
    risk: RiskLevel,
    snapshot_policy: SnapshotPolicy,
    applicable_snapshot_plugins: &[&'static str],
) -> PolicyDecision {
    let assessment = effect_opaque_assessment(risk);
    evaluate_policy(PolicyInput {
        assessment: &assessment,
        mode: Mode::Protect,
        ci_state: PolicyCiState { detected: false },
        allowlist: PolicyAllowlistResult { matched: false },
        blocklist: PolicyBlocklistResult { matched: false },
        config_flags: PolicyConfigFlags {
            ci_policy: CiPolicy::Allow,
            allowlist_override_level: AllowlistOverrideLevel::Never,
            snapshot_policy,
        },
        execution_context: PolicyExecutionContext {
            transport: ExecutionTransport::Shell,
            applicable_snapshot_plugins,
        },
        rules: PolicyRulesResult::default(),
    })
}

#[test]
fn effect_opaque_safe_command_requires_recovery_under_selective_policy() {
    let decision = evaluate_effect_opaque(RiskLevel::Safe, SnapshotPolicy::Selective, &["git"]);

    assert_eq!(decision.decision, PolicyAction::AutoApprove);
    assert_eq!(decision.rationale, PolicyRationale::SafeCommand);
    assert!(!decision.requires_confirmation);
    assert!(
        decision.snapshots_required,
        "effect-opaque Safe command must request a recovery snapshot"
    );
}

#[test]
fn effect_opaque_recovery_respects_snapshot_policy_none_opt_out() {
    let decision = evaluate_effect_opaque(RiskLevel::Safe, SnapshotPolicy::None, &["git"]);

    assert!(
        !decision.snapshots_required,
        "SnapshotPolicy::None must suppress even effect-opaque recovery"
    );
}

#[test]
fn effect_opaque_recovery_remains_required_without_applicable_plugins() {
    let decision = evaluate_effect_opaque(RiskLevel::Safe, SnapshotPolicy::Full, &[]);
    assert!(decision.snapshots_required);
}
#[test]
fn strict_effect_opaque_recovery_remains_required_without_applicable_plugins() {
    let assessment = effect_opaque_assessment(RiskLevel::Safe);
    let decision = evaluate_policy(PolicyInput {
        assessment: &assessment,
        mode: Mode::Strict,
        ci_state: PolicyCiState { detected: false },
        allowlist: PolicyAllowlistResult { matched: false },
        blocklist: PolicyBlocklistResult { matched: false },
        config_flags: PolicyConfigFlags {
            ci_policy: CiPolicy::Block,
            allowlist_override_level: AllowlistOverrideLevel::Never,
            snapshot_policy: SnapshotPolicy::Full,
        },
        execution_context: PolicyExecutionContext {
            transport: ExecutionTransport::Shell,
            applicable_snapshot_plugins: &[],
        },
        rules: PolicyRulesResult::default(),
    });

    assert!(decision.snapshots_required);
}
#[test]
fn audit_policy_rule_allow_does_not_reactivate_effect_opaque_recovery() {
    let assessment = effect_opaque_assessment(RiskLevel::Safe);
    let decision = evaluate_policy(PolicyInput {
        assessment: &assessment,
        mode: Mode::Audit,
        ci_state: PolicyCiState { detected: false },
        allowlist: PolicyAllowlistResult { matched: false },
        blocklist: PolicyBlocklistResult { matched: false },
        config_flags: PolicyConfigFlags {
            ci_policy: CiPolicy::Allow,
            allowlist_override_level: AllowlistOverrideLevel::Never,
            snapshot_policy: SnapshotPolicy::Full,
        },
        execution_context: PolicyExecutionContext {
            transport: ExecutionTransport::Shell,
            applicable_snapshot_plugins: &["git"],
        },
        rules: PolicyRulesResult {
            matched: true,
            decision: Some(aegis_types::PolicyRuleDecision::Allow),
            justification: None,
        },
    });

    assert!(!decision.snapshots_required);
}
#[test]
fn policy_rule_allow_cannot_waive_effect_opaque_recovery() {
    let assessment = effect_opaque_assessment(RiskLevel::Safe);
    let decision = evaluate_policy(PolicyInput {
        assessment: &assessment,
        mode: Mode::Protect,
        ci_state: PolicyCiState { detected: false },
        allowlist: PolicyAllowlistResult { matched: false },
        blocklist: PolicyBlocklistResult { matched: false },
        config_flags: PolicyConfigFlags {
            ci_policy: CiPolicy::Allow,
            allowlist_override_level: AllowlistOverrideLevel::Never,
            snapshot_policy: SnapshotPolicy::Selective,
        },
        execution_context: PolicyExecutionContext {
            transport: ExecutionTransport::Shell,
            applicable_snapshot_plugins: &[],
        },
        rules: PolicyRulesResult {
            matched: true,
            decision: Some(aegis_types::PolicyRuleDecision::Allow),
            justification: None,
        },
    });

    assert!(decision.snapshots_required);
}
#[test]
fn protect_warn_allowlist_cannot_waive_effect_opaque_recovery() {
    let assessment = effect_opaque_assessment(RiskLevel::Warn);
    let decision = evaluate_policy(PolicyInput {
        assessment: &assessment,
        mode: Mode::Protect,
        ci_state: PolicyCiState { detected: false },
        allowlist: PolicyAllowlistResult { matched: true },
        blocklist: PolicyBlocklistResult { matched: false },
        config_flags: PolicyConfigFlags {
            ci_policy: CiPolicy::Allow,
            allowlist_override_level: AllowlistOverrideLevel::Warn,
            snapshot_policy: SnapshotPolicy::Selective,
        },
        execution_context: PolicyExecutionContext {
            transport: ExecutionTransport::Shell,
            applicable_snapshot_plugins: &[],
        },
        rules: PolicyRulesResult::default(),
    });

    assert!(decision.snapshots_required);
}

#[test]
fn policy_decision_carries_confinement_required_axis_defaulting_false() {
    let decision = evaluate(EvalInput {
        risk: RiskLevel::Danger,
        mode: Mode::Protect,
        ci_detected: false,
        ci_policy: CiPolicy::Block,
        blocklist_matched: false,
        allowlist_matched: false,
        allowlist_override_level: AllowlistOverrideLevel::Never,
        snapshot_policy: SnapshotPolicy::Selective,
        applicable_snapshot_plugins: &["git"],
    });

    assert!(!decision.confinement_required);
}

#[test]
fn protect_danger_does_not_request_snapshots_when_policy_disables_them() {
    let decision = evaluate(EvalInput {
        risk: RiskLevel::Danger,
        mode: Mode::Protect,
        ci_detected: false,
        ci_policy: CiPolicy::Block,
        blocklist_matched: false,
        allowlist_matched: false,
        allowlist_override_level: AllowlistOverrideLevel::Never,
        snapshot_policy: SnapshotPolicy::None,
        applicable_snapshot_plugins: &["git"],
    });

    assert_decision(
        decision,
        PolicyAction::Prompt,
        PolicyRationale::RequiresConfirmation,
        true,
        false,
        false,
        None,
    );
}

#[test]
fn protect_danger_does_not_request_snapshots_without_applicable_plugins() {
    let decision = evaluate(EvalInput {
        risk: RiskLevel::Danger,
        mode: Mode::Protect,
        ci_detected: false,
        ci_policy: CiPolicy::Block,
        blocklist_matched: false,
        allowlist_matched: false,
        allowlist_override_level: AllowlistOverrideLevel::Never,
        snapshot_policy: SnapshotPolicy::Selective,
        applicable_snapshot_plugins: &[],
    });

    assert_decision(
        decision,
        PolicyAction::Prompt,
        PolicyRationale::RequiresConfirmation,
        true,
        false,
        false,
        None,
    );
}

#[test]
fn protect_ci_policy_blocks_without_confirmation() {
    let decision = evaluate(EvalInput {
        risk: RiskLevel::Warn,
        mode: Mode::Protect,
        ci_detected: true,
        ci_policy: CiPolicy::Block,
        blocklist_matched: false,
        allowlist_matched: false,
        allowlist_override_level: AllowlistOverrideLevel::Never,
        snapshot_policy: SnapshotPolicy::Selective,
        applicable_snapshot_plugins: &["git"],
    });

    assert_decision(
        decision,
        PolicyAction::Block,
        PolicyRationale::ProtectCiPolicy,
        false,
        false,
        false,
        Some(BlockReason::ProtectCiPolicy),
    );
}

#[test]
fn protect_ci_block_still_respects_danger_allowlist_override() {
    let decision = evaluate(EvalInput {
        risk: RiskLevel::Danger,
        mode: Mode::Protect,
        ci_detected: true,
        ci_policy: CiPolicy::Block,
        blocklist_matched: false,
        allowlist_matched: true,
        allowlist_override_level: AllowlistOverrideLevel::Danger,
        snapshot_policy: SnapshotPolicy::Full,
        applicable_snapshot_plugins: &["git"],
    });

    assert_decision(
        decision,
        PolicyAction::AutoApprove,
        PolicyRationale::AllowlistOverride,
        false,
        true,
        true,
        None,
    );
}

#[test]
fn strict_mode_blocks_warn_without_override() {
    let decision = evaluate(EvalInput {
        risk: RiskLevel::Warn,
        mode: Mode::Strict,
        ci_detected: false,
        ci_policy: CiPolicy::Allow,
        blocklist_matched: false,
        allowlist_matched: false,
        allowlist_override_level: AllowlistOverrideLevel::Never,
        snapshot_policy: SnapshotPolicy::Selective,
        applicable_snapshot_plugins: &["git"],
    });

    assert_decision(
        decision,
        PolicyAction::Block,
        PolicyRationale::StrictPolicy,
        false,
        false,
        false,
        Some(BlockReason::StrictPolicy),
    );
}

#[test]
fn strict_language_aware_warn_requires_analysis_override() {
    let assessment = language_aware_assessment(RiskLevel::Warn);
    let decision = evaluate_policy(PolicyInput {
        assessment: &assessment,
        mode: Mode::Strict,
        ci_state: PolicyCiState { detected: false },
        allowlist: PolicyAllowlistResult { matched: false },
        blocklist: PolicyBlocklistResult { matched: false },
        config_flags: PolicyConfigFlags {
            ci_policy: CiPolicy::Allow,
            allowlist_override_level: AllowlistOverrideLevel::Never,
            snapshot_policy: SnapshotPolicy::Selective,
        },
        execution_context: PolicyExecutionContext {
            transport: ExecutionTransport::Shell,
            applicable_snapshot_plugins: &[],
        },
        rules: PolicyRulesResult::default(),
    });

    assert_decision(
        decision,
        PolicyAction::Prompt,
        PolicyRationale::AnalysisOverrideRequired,
        true,
        false,
        false,
        None,
    );
}

#[test]
fn strict_degraded_language_aware_safe_requires_analysis_override() {
    let assessment = degraded_language_aware_assessment(RiskLevel::Safe);
    let decision = evaluate_policy(PolicyInput {
        assessment: &assessment,
        mode: Mode::Strict,
        ci_state: PolicyCiState { detected: false },
        allowlist: PolicyAllowlistResult { matched: false },
        blocklist: PolicyBlocklistResult { matched: false },
        config_flags: PolicyConfigFlags {
            ci_policy: CiPolicy::Allow,
            allowlist_override_level: AllowlistOverrideLevel::Never,
            snapshot_policy: SnapshotPolicy::Selective,
        },
        execution_context: PolicyExecutionContext {
            transport: ExecutionTransport::Shell,
            applicable_snapshot_plugins: &[],
        },
        rules: PolicyRulesResult::default(),
    });

    assert_decision(
        decision,
        PolicyAction::Prompt,
        PolicyRationale::AnalysisOverrideRequired,
        true,
        false,
        false,
        None,
    );
}

#[test]
fn strict_policy_rule_allow_for_language_aware_warn_requires_analysis_override() {
    let assessment = language_aware_assessment(RiskLevel::Warn);
    let decision = evaluate_policy(PolicyInput {
        assessment: &assessment,
        mode: Mode::Strict,
        ci_state: PolicyCiState { detected: false },
        allowlist: PolicyAllowlistResult { matched: false },
        blocklist: PolicyBlocklistResult { matched: false },
        config_flags: PolicyConfigFlags {
            ci_policy: CiPolicy::Allow,
            allowlist_override_level: AllowlistOverrideLevel::Never,
            snapshot_policy: SnapshotPolicy::Selective,
        },
        execution_context: PolicyExecutionContext {
            transport: ExecutionTransport::Shell,
            applicable_snapshot_plugins: &[],
        },
        rules: PolicyRulesResult {
            matched: true,
            decision: Some(aegis_types::PolicyRuleDecision::Allow),
            justification: None,
        },
    });

    assert_decision(
        decision,
        PolicyAction::Prompt,
        PolicyRationale::AnalysisOverrideRequired,
        true,
        false,
        false,
        None,
    );
}

#[test]
fn strict_policy_rule_prompt_for_language_aware_warn_requires_analysis_override() {
    let assessment = language_aware_assessment(RiskLevel::Warn);
    let decision = evaluate_policy(PolicyInput {
        assessment: &assessment,
        mode: Mode::Strict,
        ci_state: PolicyCiState { detected: false },
        allowlist: PolicyAllowlistResult { matched: false },
        blocklist: PolicyBlocklistResult { matched: false },
        config_flags: PolicyConfigFlags {
            ci_policy: CiPolicy::Allow,
            allowlist_override_level: AllowlistOverrideLevel::Never,
            snapshot_policy: SnapshotPolicy::Selective,
        },
        execution_context: PolicyExecutionContext {
            transport: ExecutionTransport::Shell,
            applicable_snapshot_plugins: &[],
        },
        rules: PolicyRulesResult {
            matched: true,
            decision: Some(aegis_types::PolicyRuleDecision::Prompt),
            justification: None,
        },
    });

    assert_decision(
        decision,
        PolicyAction::Prompt,
        PolicyRationale::AnalysisOverrideRequired,
        true,
        false,
        false,
        None,
    );
}

#[test]
fn strict_language_aware_danger_requires_override_and_snapshots() {
    let assessment = language_aware_assessment(RiskLevel::Danger);
    let decision = evaluate_policy(PolicyInput {
        assessment: &assessment,
        mode: Mode::Strict,
        ci_state: PolicyCiState { detected: false },
        allowlist: PolicyAllowlistResult { matched: true },
        blocklist: PolicyBlocklistResult { matched: false },
        config_flags: PolicyConfigFlags {
            ci_policy: CiPolicy::Allow,
            allowlist_override_level: AllowlistOverrideLevel::Danger,
            snapshot_policy: SnapshotPolicy::Full,
        },
        execution_context: PolicyExecutionContext {
            transport: ExecutionTransport::Shell,
            applicable_snapshot_plugins: &["git"],
        },
        rules: PolicyRulesResult::default(),
    });

    assert_decision(
        decision,
        PolicyAction::Prompt,
        PolicyRationale::AnalysisOverrideRequired,
        true,
        true,
        false,
        None,
    );
}

#[test]
fn strict_allowlist_override_danger_autoapproves_and_keeps_snapshot_requirement() {
    let decision = evaluate(EvalInput {
        risk: RiskLevel::Danger,
        mode: Mode::Strict,
        ci_detected: false,
        ci_policy: CiPolicy::Block,
        blocklist_matched: false,
        allowlist_matched: true,
        allowlist_override_level: AllowlistOverrideLevel::Danger,
        snapshot_policy: SnapshotPolicy::Full,
        applicable_snapshot_plugins: &["git"],
    });

    assert_decision(
        decision,
        PolicyAction::AutoApprove,
        PolicyRationale::AllowlistOverride,
        false,
        true,
        true,
        None,
    );
}

#[test]
fn block_risk_is_never_bypassable() {
    let decision = evaluate(EvalInput {
        risk: RiskLevel::Block,
        mode: Mode::Strict,
        ci_detected: false,
        ci_policy: CiPolicy::Allow,
        blocklist_matched: false,
        allowlist_matched: true,
        allowlist_override_level: AllowlistOverrideLevel::Danger,
        snapshot_policy: SnapshotPolicy::Full,
        applicable_snapshot_plugins: &["git"],
    });

    assert_decision(
        decision,
        PolicyAction::Block,
        PolicyRationale::IntrinsicRiskBlock,
        false,
        false,
        false,
        Some(BlockReason::IntrinsicRiskBlock),
    );
}

#[test]
fn audit_mode_cannot_bypass_intrinsic_block() {
    let decision = evaluate(EvalInput {
        risk: RiskLevel::Block,
        mode: Mode::Audit,
        ci_detected: false,
        ci_policy: CiPolicy::Allow,
        blocklist_matched: false,
        allowlist_matched: true,
        allowlist_override_level: AllowlistOverrideLevel::Danger,
        snapshot_policy: SnapshotPolicy::None,
        applicable_snapshot_plugins: &[],
    });

    assert_decision(
        decision,
        PolicyAction::Block,
        PolicyRationale::IntrinsicRiskBlock,
        false,
        false,
        false,
        Some(BlockReason::IntrinsicRiskBlock),
    );
}

#[test]
fn strict_intrinsic_block_with_language_aware_analysis_remains_terminal() {
    let assessment = language_aware_assessment(RiskLevel::Block);
    let decision = evaluate_policy(PolicyInput {
        assessment: &assessment,
        mode: Mode::Strict,
        ci_state: PolicyCiState { detected: false },
        allowlist: PolicyAllowlistResult { matched: true },
        blocklist: PolicyBlocklistResult { matched: false },
        config_flags: PolicyConfigFlags {
            ci_policy: CiPolicy::Allow,
            allowlist_override_level: AllowlistOverrideLevel::Danger,
            snapshot_policy: SnapshotPolicy::Selective,
        },
        execution_context: PolicyExecutionContext {
            transport: ExecutionTransport::Shell,
            applicable_snapshot_plugins: &[],
        },
        rules: PolicyRulesResult {
            matched: true,
            decision: Some(aegis_types::PolicyRuleDecision::Allow),
            justification: None,
        },
    });

    assert_decision(
        decision,
        PolicyAction::Block,
        PolicyRationale::IntrinsicRiskBlock,
        false,
        false,
        false,
        Some(BlockReason::IntrinsicRiskBlock),
    );
}

#[test]
fn blocklist_override_blocks_in_protect_mode() {
    let decision = evaluate(EvalInput {
        risk: RiskLevel::Warn,
        mode: Mode::Protect,
        ci_detected: false,
        ci_policy: CiPolicy::Block,
        blocklist_matched: true,
        allowlist_matched: true,
        allowlist_override_level: AllowlistOverrideLevel::Warn,
        snapshot_policy: SnapshotPolicy::Selective,
        applicable_snapshot_plugins: &["git"],
    });

    assert_decision(
        decision,
        PolicyAction::Block,
        PolicyRationale::BlocklistOverride,
        false,
        false,
        false,
        Some(BlockReason::BlocklistOverride),
    );
}

#[test]
fn blocklist_override_blocks_in_strict_mode() {
    let decision = evaluate(EvalInput {
        risk: RiskLevel::Danger,
        mode: Mode::Strict,
        ci_detected: false,
        ci_policy: CiPolicy::Allow,
        blocklist_matched: true,
        allowlist_matched: false,
        allowlist_override_level: AllowlistOverrideLevel::Never,
        snapshot_policy: SnapshotPolicy::Full,
        applicable_snapshot_plugins: &["git"],
    });

    assert_decision(
        decision,
        PolicyAction::Block,
        PolicyRationale::BlocklistOverride,
        false,
        false,
        false,
        Some(BlockReason::BlocklistOverride),
    );
}

#[test]
fn audit_mode_precedes_blocklist_for_safe_commands() {
    let decision = evaluate(EvalInput {
        risk: RiskLevel::Safe,
        mode: Mode::Audit,
        ci_detected: false,
        ci_policy: CiPolicy::Allow,
        blocklist_matched: true,
        allowlist_matched: false,
        allowlist_override_level: AllowlistOverrideLevel::Warn,
        snapshot_policy: SnapshotPolicy::Selective,
        applicable_snapshot_plugins: &["git"],
    });

    assert_decision(
        decision,
        PolicyAction::AutoApprove,
        PolicyRationale::AuditMode,
        false,
        false,
        false,
        None,
    );
}

fn evaluate_with_rules(
    risk: RiskLevel,
    mode: Mode,
    rules_matched: bool,
    rules_decision: Option<aegis_types::PolicyRuleDecision>,
) -> PolicyDecision {
    let assessment = assessment(risk);
    evaluate_policy(PolicyInput {
        assessment: &assessment,
        mode,
        ci_state: PolicyCiState { detected: false },
        allowlist: PolicyAllowlistResult { matched: false },
        blocklist: PolicyBlocklistResult { matched: false },
        config_flags: PolicyConfigFlags {
            ci_policy: CiPolicy::Allow,
            allowlist_override_level: AllowlistOverrideLevel::Never,
            snapshot_policy: SnapshotPolicy::None,
        },
        execution_context: PolicyExecutionContext {
            transport: ExecutionTransport::Shell,
            applicable_snapshot_plugins: &[],
        },
        rules: PolicyRulesResult {
            matched: rules_matched,
            decision: rules_decision,
            justification: None,
        },
    })
}

#[test]
fn test_rules_allow_overrides_protect_mode_warn() {
    let decision = evaluate_with_rules(
        RiskLevel::Warn,
        Mode::Protect,
        true,
        Some(aegis_types::PolicyRuleDecision::Allow),
    );

    assert_eq!(decision.decision, PolicyAction::AutoApprove);
    assert_eq!(decision.rationale, PolicyRationale::PolicyRulesOverride);
}

#[test]
fn test_rules_block_overrides_safe_command() {
    let decision = evaluate_with_rules(
        RiskLevel::Safe,
        Mode::Protect,
        true,
        Some(aegis_types::PolicyRuleDecision::Block),
    );

    assert_eq!(decision.decision, PolicyAction::Block);
    assert_eq!(decision.rationale, PolicyRationale::PolicyRulesOverride);
    assert_eq!(
        decision.block_reason(),
        Some(BlockReason::PolicyRulesOverride)
    );
}

#[test]
fn test_rules_prompt_overrides_safe_command() {
    let decision = evaluate_with_rules(
        RiskLevel::Safe,
        Mode::Protect,
        true,
        Some(aegis_types::PolicyRuleDecision::Prompt),
    );

    assert_eq!(decision.decision, PolicyAction::Prompt);
    assert_eq!(decision.rationale, PolicyRationale::PolicyRulesOverride);
}

#[test]
fn test_rules_not_matched_falls_through_to_normal_policy() {
    let decision = evaluate_with_rules(RiskLevel::Safe, Mode::Protect, false, None);

    assert_eq!(decision.decision, PolicyAction::AutoApprove);
    assert_eq!(decision.rationale, PolicyRationale::SafeCommand);
}

fn action_for(
    risk: RiskLevel,
    mode: Mode,
    ci_detected: bool,
    ci_policy: CiPolicy,
    allowlist_matched: bool,
    allowlist_override_level: AllowlistOverrideLevel,
) -> PolicyAction {
    evaluate(EvalInput {
        risk,
        mode,
        ci_detected,
        ci_policy,
        blocklist_matched: false,
        allowlist_matched,
        allowlist_override_level,
        snapshot_policy: SnapshotPolicy::Selective,
        applicable_snapshot_plugins: &[],
    })
    .decision
}

#[test]
fn ci_policy_block_blocks_non_safe_risks_and_spares_safe_in_ci() {
    for (risk, expected) in [
        (RiskLevel::Warn, PolicyAction::Block),
        (RiskLevel::Danger, PolicyAction::Block),
        (RiskLevel::Block, PolicyAction::Block),
        (RiskLevel::Safe, PolicyAction::AutoApprove),
    ] {
        let action = action_for(
            risk,
            Mode::Protect,
            true,
            CiPolicy::Block,
            false,
            AllowlistOverrideLevel::Never,
        );
        assert_eq!(action, expected, "{risk:?}");
    }
}

#[test]
fn safe_command_autoapproves_regardless_of_ci_state_or_policy() {
    for (ci_detected, ci_policy) in [
        (true, CiPolicy::Allow),
        (false, CiPolicy::Allow),
        (false, CiPolicy::Block),
    ] {
        let action = action_for(
            RiskLevel::Safe,
            Mode::Protect,
            ci_detected,
            ci_policy,
            false,
            AllowlistOverrideLevel::Never,
        );
        assert_eq!(action, PolicyAction::AutoApprove);
    }
}

#[test]
fn strict_allowlisted_warn_follows_override_level() {
    for (level, expected) in [
        (AllowlistOverrideLevel::Warn, PolicyAction::AutoApprove),
        (AllowlistOverrideLevel::Never, PolicyAction::Block),
    ] {
        let action = action_for(
            RiskLevel::Warn,
            Mode::Strict,
            false,
            CiPolicy::Allow,
            true,
            level,
        );
        assert_eq!(action, expected, "{level:?}");
    }
}

#[test]
fn rules_allow_cannot_bypass_block_risk_level() {
    let decision = evaluate_with_rules(
        RiskLevel::Block,
        Mode::Protect,
        true,
        Some(aegis_types::PolicyRuleDecision::Allow),
    );

    assert_eq!(decision.decision, PolicyAction::Block);
    assert_eq!(decision.rationale, PolicyRationale::IntrinsicRiskBlock);
    assert_eq!(
        decision.block_reason(),
        Some(BlockReason::IntrinsicRiskBlock)
    );
}
