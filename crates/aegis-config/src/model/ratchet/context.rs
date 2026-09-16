//! Values a `Custom` direction needs from the trusted base, computed before
//! the base is destructured and its fields move.

use super::super::{AegisConfig, PolicyRule, PolicyRuleDecision, SnapshotPolicy};

/// Whether a built-in Snapshot provider is enabled in `base`. Under
/// `SnapshotPolicy::None` the registry materializes NO providers, so nothing
/// is ratcheted. Under `SnapshotPolicy::Full` the registry materializes every
/// built-in provider regardless of the per-plugin flags, so `Full` counts as
/// every provider enabled. Under `SnapshotPolicy::Selective` only providers
/// whose `auto_snapshot_*` flag is set are enabled.
pub(crate) fn provider_enabled_in_base(base: &AegisConfig, auto_snapshot_flag: bool) -> bool {
    base.snapshot_policy != SnapshotPolicy::None
        && (base.snapshot_policy == SnapshotPolicy::Full || auto_snapshot_flag)
}

/// Per-provider "is this provider's target protected" predicates, computed
/// from `base` before `model::merge_layer` destructures it. The provider
/// target `Custom` rules (`sqlite_snapshot_path`, `postgres_snapshot`,
/// `mysql_snapshot`, `supabase_snapshot`, `docker_scope`) read these instead
/// of recomputing them mid-merge, keeping them in lock-step with each other
/// (#269 C3-01).
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

/// Predicate identifying a project-layer `[[rules]]` entry that attempts to
/// auto-approve (`decision = "Allow"`). Such entries are DROPPED at the
/// project merge (the project layer may only tighten via Prompt/Block, never
/// auto-approve) and surfaced as a ratchet warning. Global-layer Allow
/// entries are NOT filtered (global is trusted, last-wins).
pub(crate) fn is_untrusted_allow(rule: &PolicyRule) -> bool {
    // A project-layer rule is an untrusted auto-approve if EITHER its top-level
    // `decision = "Allow"` OR its `when.then = "Allow"` — at runtime
    // `effective_decision` returns `when.then` when the env condition matches,
    // so a `decision = "prompt"` (or `"block"`) rule with `when.then = "allow"`
    // would silently auto-approve.
    rule.decision == PolicyRuleDecision::Allow
        || rule
            .when
            .as_ref()
            .is_some_and(|w| w.then == PolicyRuleDecision::Allow)
}
