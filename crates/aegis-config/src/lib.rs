pub mod allowlist;
pub mod amend;
mod atomic;
pub mod ceiling;
pub mod error;
mod home;
pub mod model;
pub mod pattern_match;
pub mod snapshot;
pub mod validate;

pub use atomic::write_atomic;
pub use error::ConfigError;
pub use home::home_dir;

#[cfg(test)]
pub use allowlist::analyze_blocklist_rule;
pub use allowlist::{
    Allowlist, AllowlistContext, AllowlistMatch, AllowlistWarning, Blocklist, BlocklistMatch,
    BlocklistWarning, ConfigSourceLayer, LayeredAllowlistRule, LayeredBlocklistRule,
    analyze_allowlist_rule,
};
pub use amend::{
    AppendOutcome, active_config_path_for_append, append_allow_rule, append_block_rule,
};
pub use model::{
    AegisConfig, AllowlistRule, AuditConfig, AuditIntegrityMode, BlockRule, PolicyPatternToken,
    PolicyRule, PruneConfig, SandboxSettings, UserPattern, WhenClause,
};
pub use pattern_match::policy_pattern_matches;
pub use snapshot::{
    DockerScope, DockerScopeMode, MysqlSnapshotConfig, PostgresSnapshotConfig,
    SupabaseSnapshotConfig,
};
#[cfg(test)]
pub use validate::validation_load_error;
pub use validate::{
    ConfigSourceMap, ValidationIssue, ValidationReport, locate_invalid_custom_pattern,
    reject_policy_star, reject_policy_star_at, validate_config, validate_config_layers,
    validate_policy_rules,
};
