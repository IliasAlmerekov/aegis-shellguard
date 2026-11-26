use super::*;

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
