use super::*;

// ── ADR-031 §1: auto-approved Danger carries a Required recovery obligation ──

fn allowlisted_danger(
    mode: Mode,
    snapshot_policy: SnapshotPolicy,
    applicable_snapshot_plugins: &[&'static str],
) -> PolicyDecision {
    evaluate(EvalInput {
        risk: RiskLevel::Danger,
        mode,
        ci_detected: false,
        ci_policy: CiPolicy::Allow,
        blocklist_matched: false,
        allowlist_matched: true,
        allowlist_override_level: AllowlistOverrideLevel::Danger,
        snapshot_policy,
        applicable_snapshot_plugins,
    })
}

#[test]
fn protect_allowlisted_danger_requires_snapshots_without_applicable_plugins() {
    let decision = allowlisted_danger(Mode::Protect, SnapshotPolicy::Selective, &[]);

    assert_eq!(decision.decision, PolicyAction::AutoApprove);
    assert!(decision.snapshots_required);
}

#[test]
fn strict_allowlisted_danger_requires_snapshots_without_applicable_plugins() {
    let decision = allowlisted_danger(Mode::Strict, SnapshotPolicy::Selective, &[]);

    assert_eq!(decision.decision, PolicyAction::AutoApprove);
    assert!(decision.snapshots_required);
}

#[test]
fn policy_rule_allow_danger_requires_snapshots_without_applicable_plugins() {
    let assessment = assessment(RiskLevel::Danger);
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

    assert_eq!(decision.decision, PolicyAction::AutoApprove);
    assert!(decision.snapshots_required);
}

#[test]
fn allowlisted_danger_creates_no_obligation_under_snapshot_policy_none() {
    let decision = allowlisted_danger(Mode::Protect, SnapshotPolicy::None, &[]);

    assert!(!decision.snapshots_required);
}

#[test]
fn audit_allowlisted_danger_creates_no_obligation_without_applicable_plugins() {
    let decision = allowlisted_danger(Mode::Audit, SnapshotPolicy::Selective, &[]);

    assert!(!decision.snapshots_required);
}
