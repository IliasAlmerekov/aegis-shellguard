use super::super::{AegisConfig, PolicyRule, PolicyRuleDecision, SnapshotPolicy};

pub(crate) fn provider_enabled_in_base(base: &AegisConfig, auto_snapshot_flag: bool) -> bool {
    base.snapshot_policy != SnapshotPolicy::None
        && (base.snapshot_policy == SnapshotPolicy::Full || auto_snapshot_flag)
}

pub(crate) struct RatchetContext {
    pub(crate) postgres_enabled: bool,
    pub(crate) mysql_enabled: bool,
    pub(crate) supabase_enabled: bool,
    pub(crate) sqlite_enabled: bool,
    pub(crate) docker_enabled: bool,
}

impl RatchetContext {
    pub(crate) fn compute(base: &AegisConfig) -> Self {
        Self {
            postgres_enabled: provider_enabled_in_base(base, base.auto_snapshot_postgres),
            mysql_enabled: provider_enabled_in_base(base, base.auto_snapshot_mysql),
            supabase_enabled: provider_enabled_in_base(base, base.auto_snapshot_supabase),
            sqlite_enabled: provider_enabled_in_base(base, base.auto_snapshot_sqlite),
            docker_enabled: provider_enabled_in_base(base, base.auto_snapshot_docker),
        }
    }
}

pub(crate) fn is_untrusted_allow(rule: &PolicyRule) -> bool {
    rule.decision == PolicyRuleDecision::Allow
        || rule
            .when
            .as_ref()
            .is_some_and(|w| w.then == PolicyRuleDecision::Allow)
}
