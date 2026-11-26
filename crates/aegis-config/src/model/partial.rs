use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::ConfigError;

use super::serde_helpers::{deserialize_allowlist_rules, deserialize_optional_config_version};
use super::{
    AllowlistOverrideLevel, AllowlistRule, AuditIntegrityMode, BlockRule, CiPolicy, DockerScope,
    Mode, MysqlSnapshotConfig, PolicyRule, PostgresSnapshotConfig, SnapshotPolicy, TrustedAlias,
    UserPattern,
};

type Result<T> = std::result::Result<T, ConfigError>;

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct PartialPruneConfig {
    pub(super) enabled: Option<bool>,
    pub(super) max_count_per_provider: Option<usize>,
    pub(super) max_age_days: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct PartialSupabaseDb {
    pub(super) database: Option<String>,
    pub(super) host: Option<String>,
    pub(super) port: Option<u16>,
    pub(super) user: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct PartialSupabaseSnapshotConfig {
    pub(super) project_ref: Option<String>,
    pub(super) require_config_target_match_on_rollback: Option<bool>,
    pub(super) db: PartialSupabaseDb,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct PartialSandboxSettings {
    pub(super) allow_write: Option<Vec<PathBuf>>,
    pub(super) allow_network: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct PartialLanguageAnalysisConfig {
    pub(super) inline_source_limit_bytes: Option<u64>,
    pub(super) script_file_limit_bytes: Option<u64>,
    pub(super) max_script_files: Option<u64>,
    pub(super) max_depth: Option<u64>,
    pub(super) max_targets: Option<u64>,
    pub(super) max_aggregate_bytes: Option<u64>,
    pub(super) timeout_ms: Option<u64>,
    pub(super) trusted_aliases: Option<Vec<TrustedAlias>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct PartialConfig {
    #[serde(default, deserialize_with = "deserialize_optional_config_version")]
    pub(super) config_version: Option<u32>,
    pub(super) mode: Option<Mode>,
    pub(super) custom_patterns: Vec<UserPattern>,
    #[serde(
        default,
        rename = "allow",
        alias = "allowlist",
        deserialize_with = "deserialize_allowlist_rules"
    )]
    pub(super) allowlist: Vec<AllowlistRule>,
    #[serde(default, rename = "block", alias = "blocklist")]
    pub(super) blocklist: Vec<BlockRule>,
    pub(super) allowlist_override_level: Option<AllowlistOverrideLevel>,
    pub(super) snapshot_policy: Option<SnapshotPolicy>,
    pub(super) auto_snapshot_git: Option<bool>,
    pub(super) auto_snapshot_docker: Option<bool>,
    pub(super) auto_snapshot_postgres: Option<bool>,
    pub(super) postgres_snapshot: Option<PostgresSnapshotConfig>,
    pub(super) auto_snapshot_mysql: Option<bool>,
    pub(super) mysql_snapshot: Option<MysqlSnapshotConfig>,
    pub(super) auto_snapshot_supabase: Option<bool>,
    pub(super) supabase_snapshot: PartialSupabaseSnapshotConfig,
    pub(super) auto_snapshot_sqlite: Option<bool>,
    pub(super) sqlite_snapshot_path: Option<String>,
    pub(super) docker_scope: Option<DockerScope>,
    pub(super) ci_policy: Option<CiPolicy>,
    pub(super) audit: PartialAuditConfig,
    #[serde(default, rename = "rules")]
    pub(super) rules: Vec<PolicyRule>,
    pub(super) sandbox: PartialSandboxSettings,
    pub(super) prune: PartialPruneConfig,
    pub(super) language_analysis: PartialLanguageAnalysisConfig,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct PartialAuditConfig {
    pub(super) rotation_enabled: Option<bool>,
    pub(super) max_file_size_bytes: Option<u64>,
    pub(super) retention_files: Option<usize>,
    pub(super) compress_rotated: Option<bool>,
    pub(super) integrity_mode: Option<AuditIntegrityMode>,
}

impl PartialConfig {
    pub(super) fn from_path(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)?;
        let config: Self = toml::from_str(&contents).map_err(|error| ConfigError::ParseFailed {
            path: path.display().to_string(),
            source: Box::new(error),
        })?;

        Ok(config)
    }
}
