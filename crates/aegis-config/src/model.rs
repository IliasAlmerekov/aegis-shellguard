use std::env;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::allowlist::{
    Allowlist, Blocklist, ConfigSourceLayer, LayeredAllowlistRule, LayeredBlocklistRule,
};
use super::snapshot::{
    DockerScope, MysqlSnapshotConfig, PostgresSnapshotConfig, SupabaseSnapshotConfig,
};
use crate::error::ConfigError;

pub(crate) fn validate_custom_patterns(patterns: &[UserPattern]) -> Result<()> {
    if patterns.is_empty() {
        return Ok(());
    }
    let converted: Vec<aegis_types::Pattern> = patterns.iter().cloned().map(Into::into).collect();
    aegis_scanner::PatternSet::from_sources(&converted)
        .and_then(aegis_scanner::Scanner::try_new)
        .map(|_| ())
        // Fold into `Config` (not a distinct variant) so the per-file path
        // wrapping in `validate_runtime_requirements_for_path` still applies.
        .map_err(|err| ConfigError::Config(err.to_string()))
}

pub(crate) fn validate_builtin_scanner() -> Result<()> {
    aegis_scanner::PatternSet::from_sources(&[])
        .and_then(aegis_scanner::Scanner::try_new)
        .map(|_| ())
        .map_err(|err| ConfigError::Config(err.to_string()))
}

mod enums;
mod merge;
mod partial;
mod ratchet;
mod rules;
mod serde_helpers;
mod template;

use aegis_types::{AllowlistOverrideLevel, CiPolicy, Mode, PolicyRuleDecision, SnapshotPolicy};
pub use enums::AuditIntegrityMode;
pub use rules::{
    AllowlistRule, AuditConfig, BlockRule, LANGUAGE_ANALYSIS_SCRIPT_FILE_HARD_CEILING_BYTES,
    LanguageAnalysisConfig, PolicyPatternToken, PolicyRule, TrustedAlias, UserPattern, WhenClause,
};

// Bring submodule items into the `model` namespace so they remain reachable
// from `model::tests` (which does `pub use super::*`) and from the merge logic
// below. These stay private aliases — visibility is unchanged.
use partial::PartialConfig;
#[cfg(test)]
use partial::{PartialSupabaseDb, PartialSupabaseSnapshotConfig};
use serde_helpers::{
    default_config_version, deserialize_allowlist_rules, deserialize_config_version,
};
use template::PROJECT_CONFIG_FILE;

const GLOBAL_CONFIG_DIR: &str = ".config/aegis";
const GLOBAL_CONFIG_FILE: &str = "config.toml";
pub const CURRENT_CONFIG_VERSION: u32 = 1;

type Result<T> = std::result::Result<T, ConfigError>;

fn most_restrictive_mode(left: Mode, right: Mode) -> Mode {
    if mode_rank(right) >= mode_rank(left) {
        right
    } else {
        left
    }
}

fn mode_rank(mode: Mode) -> u8 {
    match mode {
        Mode::Audit => 0,
        Mode::Protect => 1,
        Mode::Strict => 2,
    }
}

fn most_restrictive_allowlist_override_level(
    left: AllowlistOverrideLevel,
    right: AllowlistOverrideLevel,
) -> AllowlistOverrideLevel {
    if allowlist_override_level_rank(right) >= allowlist_override_level_rank(left) {
        right
    } else {
        left
    }
}

fn allowlist_override_level_rank(level: AllowlistOverrideLevel) -> u8 {
    match level {
        AllowlistOverrideLevel::Danger => 0,
        AllowlistOverrideLevel::Warn => 1,
        AllowlistOverrideLevel::Never => 2,
    }
}

fn most_restrictive_ci_policy(left: CiPolicy, right: CiPolicy) -> CiPolicy {
    if ci_policy_rank(right) >= ci_policy_rank(left) {
        right
    } else {
        left
    }
}

fn ci_policy_rank(policy: CiPolicy) -> u8 {
    match policy {
        CiPolicy::Allow => 0,
        CiPolicy::Block => 1,
    }
}

fn most_restrictive_snapshot_policy(left: SnapshotPolicy, right: SnapshotPolicy) -> SnapshotPolicy {
    if snapshot_policy_rank(right) >= snapshot_policy_rank(left) {
        right
    } else {
        left
    }
}

fn snapshot_policy_rank(policy: SnapshotPolicy) -> u8 {
    match policy {
        SnapshotPolicy::None => 0,
        SnapshotPolicy::Selective => 1,
        SnapshotPolicy::Full => 2,
    }
}

pub(super) fn most_restrictive_integrity_mode(
    left: AuditIntegrityMode,
    right: AuditIntegrityMode,
) -> AuditIntegrityMode {
    if integrity_mode_rank(right) >= integrity_mode_rank(left) {
        right
    } else {
        left
    }
}

fn integrity_mode_rank(mode: AuditIntegrityMode) -> u8 {
    match mode {
        AuditIntegrityMode::Off => 0,
        AuditIntegrityMode::ChainSha256 => 1,
    }
}

// Every field's Ratchet direction lives in `model::ratchet` (ADR-013,
// CONTEXT.md "Ratchet direction"). `merge_layer` below destructures
// `AegisConfig` and every nested config struct exhaustively and routes each
// field through its declared direction in one pass, producing the merged
// config AND the project-layer weakening warnings together.
use ratchet::SecurityRatchetWarning;

#[derive(Debug, Clone)]
pub struct ConfigLayerPath {
    pub source_layer: ConfigSourceLayer,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default)]
#[serde(default, deny_unknown_fields)]
#[schemars(
    description = "Sandbox configuration — controls the Trusted ceiling for mandatory Sandbox confinement."
)]
pub struct SandboxSettings {
    #[schemars(
        description = "Deprecated: Aegis ignores this value; the Sandbox is always applied
outside `Mode::Audit` (ADR-029). Kept for schema v1 compatibility."
    )]
    #[serde(default, skip_serializing)]
    #[schemars(extend("deprecated" = true))]
    pub enabled: bool,
    #[schemars(
        description = "Deprecated: Aegis ignores this value; Sandbox unavailability always
blocks outside `Mode::Audit` (ADR-029). Kept for schema v1 compatibility."
    )]
    #[serde(default, skip_serializing)]
    #[schemars(extend("deprecated" = true))]
    pub required: bool,
    #[schemars(
        description = "Writable roots of the Trusted ceiling. Absent means the computed
default (workspace plus temporary directory); a present list replaces
that default, and an explicit `[]` means zero writable roots."
    )]
    #[schemars(with = "Option<Vec<PathBuf>>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_write: Option<Vec<crate::ceiling::WriteRoot>>,
    #[schemars(
        description = "True when `allow_write` is a project list over an absent trusted list:
it can only narrow the computed default, which is intersected at
ceiling resolution (not known at merge). Not part of the wire format."
    )]
    #[serde(skip)]
    pub allow_write_narrows_computed: bool,
    #[schemars(
        description = "The list a project `allow_write` narrowed, kept so enforcement can check
canonical containment against it. `None` when no project list narrowed
a present trusted list. Not part of the wire format."
    )]
    #[serde(skip)]
    pub allow_write_trusted: Option<Vec<crate::ceiling::WriteRoot>>,
    #[schemars(description = "Whether the sandboxed process may access the network.")]
    pub allow_network: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default)]
#[serde(default, deny_unknown_fields)]
#[schemars(description = "Prune configuration for `aegis snapshot prune`.")]
pub struct PruneConfig {
    #[schemars(
        description = "Whether prune is enabled. Currently advisory; manual `--yes` is still required."
    )]
    pub enabled: bool,
    #[schemars(
        description = "Keep the newest N snapshots per provider. `None` disables the count rule."
    )]
    pub max_count_per_provider: Option<usize>,
    #[schemars(
        description = "Keep snapshots newer than this many days. `None` disables the age rule."
    )]
    pub max_age_days: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
#[schemars(description = "Top-level Aegis runtime configuration.

Loaded in order: built-in defaults → `~/.config/aegis/config.toml` (user-global)
→ `.aegis.toml` (project). Every field's Ratchet direction — how far the
project layer may change it, if at all — is declared in `model::merge_layer`
and listed field-by-field in ADR-013's normative table.")]
pub struct AegisConfig {
    #[schemars(description = "Schema version. Must equal [`CURRENT_CONFIG_VERSION`].")]
    #[serde(
        default = "default_config_version",
        deserialize_with = "deserialize_config_version"
    )]
    pub config_version: u32,
    #[schemars(description = "Operating mode: `Protect`, `Audit`, or `Strict`.")]
    pub mode: Mode,
    #[schemars(
        description = "Extra user-defined patterns merged with built-in patterns at runtime."
    )]
    pub custom_patterns: Vec<UserPattern>,
    #[schemars(
        description = "Per-pattern provenance (which layer each `custom_patterns` entry came from). Internal; not serialized."
    )]
    #[serde(skip)]
    pub(crate) custom_pattern_layers: Vec<ConfigSourceLayer>,
    #[schemars(
        description = "`current_dir` this config was resolved against, when loaded via
[`Self::load_for`]/[`Self::load`]. Internal; not serialized."
    )]
    #[serde(skip)]
    pub(crate) source_current_dir: Option<PathBuf>,
    #[schemars(
        description = "`home_dir` this config was resolved against, when loaded via
[`Self::load_for`]/[`Self::load`]. Internal; not serialized."
    )]
    #[serde(skip)]
    pub(crate) source_home_dir: Option<PathBuf>,
    #[schemars(description = "Structured allow-list rules (TOML: `[[allow]]`).")]
    #[serde(
        default,
        rename = "allow",
        alias = "allowlist",
        deserialize_with = "deserialize_allowlist_rules"
    )]
    pub allowlist: Vec<AllowlistRule>,
    #[schemars(description = "Per-rule provenance for `allowlist`. Internal; not serialized.")]
    #[serde(skip)]
    pub(crate) allowlist_layers: Vec<ConfigSourceLayer>,
    #[schemars(description = "Structured block-list rules (TOML: `[[block]]`).")]
    #[serde(default, rename = "block", alias = "blocklist")]
    pub blocklist: Vec<BlockRule>,
    #[schemars(description = "Per-rule provenance for `blocklist`. Internal; not serialized.")]
    #[serde(skip)]
    pub(crate) blocklist_layers: Vec<ConfigSourceLayer>,
    #[schemars(
        description = "Which layer set `audit.max_file_size_bytes`. Internal; not serialized."
    )]
    #[serde(skip)]
    pub(crate) audit_max_file_size_bytes_source: Option<ConfigSourceLayer>,
    #[schemars(description = "Which layer set `audit.retention_files`. Internal; not serialized.")]
    #[serde(skip)]
    pub(crate) audit_retention_files_source: Option<ConfigSourceLayer>,
    #[schemars(
        description = "Maximum risk level the allow-list may auto-approve in Protect/Strict mode."
    )]
    pub allowlist_override_level: AllowlistOverrideLevel,
    #[schemars(description = "Controls which Snapshot plugins run for planned recovery.")]
    pub snapshot_policy: SnapshotPolicy,
    #[schemars(description = "Enable Git when a command's Snapshot plan requests it.")]
    pub auto_snapshot_git: bool,
    #[schemars(description = "Enable Docker when a command's Snapshot plan requests it.")]
    pub auto_snapshot_docker: bool,
    #[schemars(description = "Enable PostgreSQL when a command's Snapshot plan requests it.")]
    pub auto_snapshot_postgres: bool,
    #[schemars(description = "PostgreSQL connection details used when snapshotting is enabled.")]
    pub postgres_snapshot: PostgresSnapshotConfig,
    #[schemars(description = "Enable MySQL when a command's Snapshot plan requests it.")]
    pub auto_snapshot_mysql: bool,
    #[schemars(description = "MySQL connection details used when snapshotting is enabled.")]
    pub mysql_snapshot: MysqlSnapshotConfig,
    #[schemars(description = "Enable Supabase when a command's Snapshot plan requests it.")]
    pub auto_snapshot_supabase: bool,
    #[schemars(
        description = "Supabase snapshot provider settings used when snapshotting is enabled.
Layered config replaces this provider config as a whole."
    )]
    pub supabase_snapshot: SupabaseSnapshotConfig,
    #[schemars(description = "Enable SQLite when a command's Snapshot plan requests it.")]
    pub auto_snapshot_sqlite: bool,
    #[schemars(
        description = "Path to a SQLite database file, relative to the current working
directory or absolute."
    )]
    pub sqlite_snapshot_path: String,
    #[schemars(description = "Which Docker containers to include in snapshots.")]
    pub docker_scope: DockerScope,
    #[schemars(description = "Behaviour when a CI environment is detected.")]
    pub ci_policy: CiPolicy,
    #[schemars(description = "Audit log rotation and integrity settings.")]
    pub audit: AuditConfig,
    #[schemars(description = "Typed policy rules (TOML: `[[rules]]`).")]
    #[serde(default, rename = "rules")]
    pub rules: Vec<PolicyRule>,
    #[schemars(description = "Sandbox layer settings.")]
    pub sandbox: SandboxSettings,
    #[schemars(description = "Snapshot prune retention settings.")]
    pub prune: PruneConfig,
    #[schemars(description = "Language-aware analysis script-file and trusted-alias budgets.")]
    pub language_analysis: LanguageAnalysisConfig,
}

impl Default for AegisConfig {
    fn default() -> Self {
        Self::defaults()
    }
}

impl AegisConfig {
    pub fn load() -> Result<Self> {
        let current_dir = env::current_dir()?;
        let home_dir = crate::home_dir();

        Self::load_for(&current_dir, home_dir.as_deref())
    }

    pub fn load_inspection() -> Result<Self> {
        let current_dir = env::current_dir()?;
        let home_dir = crate::home_dir();

        Self::load_for_inspection(&current_dir, home_dir.as_deref())
    }

    pub fn defaults() -> Self {
        Self {
            config_version: CURRENT_CONFIG_VERSION,
            mode: Mode::Protect,
            custom_patterns: Vec::new(),
            custom_pattern_layers: Vec::new(),
            source_current_dir: None,
            source_home_dir: None,
            allowlist: Vec::new(),
            allowlist_layers: Vec::new(),
            blocklist: Vec::new(),
            blocklist_layers: Vec::new(),
            audit_max_file_size_bytes_source: None,
            audit_retention_files_source: None,
            allowlist_override_level: AllowlistOverrideLevel::Warn,
            snapshot_policy: SnapshotPolicy::Selective,
            auto_snapshot_git: true,
            auto_snapshot_docker: false,
            auto_snapshot_postgres: false,
            postgres_snapshot: PostgresSnapshotConfig::default(),
            auto_snapshot_mysql: false,
            mysql_snapshot: MysqlSnapshotConfig::default(),
            auto_snapshot_supabase: false,
            supabase_snapshot: SupabaseSnapshotConfig::default(),
            auto_snapshot_sqlite: false,
            sqlite_snapshot_path: String::new(),
            docker_scope: DockerScope::default(),
            ci_policy: CiPolicy::Block,
            audit: AuditConfig::default(),
            rules: Vec::new(),
            sandbox: SandboxSettings::default(),
            prune: PruneConfig::default(),
            language_analysis: LanguageAnalysisConfig::default(),
        }
    }

    pub fn to_toml_string(&self) -> Result<String> {
        toml::to_string_pretty(self)
            .map_err(|error| ConfigError::Config(format!("failed to serialize config: {error}")))
    }

    pub fn has_full_custom_pattern_provenance(&self) -> bool {
        !self.custom_patterns.is_empty()
            && self.custom_pattern_layers.len() == self.custom_patterns.len()
    }

    pub fn source_current_dir(&self) -> Option<&Path> {
        self.source_current_dir.as_deref()
    }

    pub fn source_home_dir(&self) -> Option<&Path> {
        self.source_home_dir.as_deref()
    }

    pub fn validate_runtime_requirements(&self) -> Result<()> {
        self.validate()?;
        Allowlist::from_layered_rules(&self.layered_allowlist_rules()).map(|_| ())?;
        Blocklist::from_layered_rules(&self.layered_blocklist_rules()).map(|_| ())?;
        Ok(())
    }

    fn validate_runtime_requirements_selective(&self) -> Result<()> {
        self.validate()?;
        Allowlist::from_layered_rules(&self.layered_allowlist_rules()).map(|_| ())?;
        Blocklist::from_layered_rules(&self.layered_blocklist_rules()).map(|_| ())?;
        Ok(())
    }

    pub fn load_for(current_dir: &Path, home_dir: Option<&Path>) -> Result<Self> {
        Self::load_for_internal(current_dir, home_dir, true)
    }

    pub fn load_for_inspection(current_dir: &Path, home_dir: Option<&Path>) -> Result<Self> {
        Self::load_for_internal(current_dir, home_dir, false)
    }

    pub fn layer_paths_for(current_dir: &Path, home_dir: Option<&Path>) -> Vec<ConfigLayerPath> {
        let global_path = home_dir.map(|h| h.join(GLOBAL_CONFIG_DIR).join(GLOBAL_CONFIG_FILE));
        let project_path = current_dir.join(PROJECT_CONFIG_FILE);
        let mut layers = Vec::new();

        if let Some(path) = global_path.filter(|path| path.is_file()) {
            layers.push(ConfigLayerPath {
                source_layer: ConfigSourceLayer::Global,
                path,
            });
        }

        if project_path.is_file() {
            layers.push(ConfigLayerPath {
                source_layer: ConfigSourceLayer::Project,
                path: project_path,
            });
        }

        layers
    }

    pub fn merge_layer_path_unvalidated(base: Self, layer: &ConfigLayerPath) -> Result<Self> {
        Self::merge_layer_path_with_warnings(base, layer).map(|(config, _)| config)
    }

    pub(crate) fn merge_layer_path_with_warnings(
        base: Self,
        layer: &ConfigLayerPath,
    ) -> Result<(Self, Vec<SecurityRatchetWarning>)> {
        let (config, warnings, _) = Self::merge_layer_path_with_all_warnings(base, layer)?;
        Ok((config, warnings))
    }

    pub(crate) fn merge_layer_path_with_all_warnings(
        base: Self,
        layer: &ConfigLayerPath,
    ) -> Result<(
        Self,
        Vec<SecurityRatchetWarning>,
        Vec<ratchet::DeprecatedSandboxFieldWarning>,
    )> {
        let overlay = PartialConfig::from_path(&layer.path)?;
        let (config, ratchet, deprecated) = Self::merge_layer_all_warnings(
            base,
            overlay,
            layer.source_layer,
            &layer.path.to_string_lossy(),
        );
        Ok((config, ratchet, deprecated))
    }

    fn load_for_internal(
        current_dir: &Path,
        home_dir: Option<&Path>,
        validate_runtime_requirements: bool,
    ) -> Result<Self> {
        let global_path = home_dir.map(|h| h.join(GLOBAL_CONFIG_DIR).join(GLOBAL_CONFIG_FILE));
        let project_path = current_dir.join(PROJECT_CONFIG_FILE);

        let mut merged = Self::defaults();

        if let Some(path) = global_path.as_deref().filter(|p| p.is_file()) {
            let global = PartialConfig::from_path(path)?;
            (merged, _) = Self::merge_layer(
                merged,
                global,
                ConfigSourceLayer::Global,
                &path.to_string_lossy(),
            );
            if validate_runtime_requirements {
                merged.validate_runtime_requirements_for_path(path)?;
            }
        }

        if project_path.is_file() {
            let project = PartialConfig::from_path(&project_path)?;
            (merged, _) = Self::merge_layer(
                merged,
                project,
                ConfigSourceLayer::Project,
                &project_path.to_string_lossy(),
            );
            if validate_runtime_requirements {
                merged.validate_runtime_requirements_for_path(&project_path)?;
            }
        }

        merged.source_current_dir = Some(current_dir.to_path_buf());
        merged.source_home_dir = home_dir.map(Path::to_path_buf);

        Ok(merged)
    }

    fn validate(&self) -> Result<()> {
        if self.audit.rotation_enabled && self.audit.max_file_size_bytes == 0 {
            return Err(ConfigError::Config(
                "audit.max_file_size_bytes must be greater than 0 when audit rotation is enabled"
                    .to_string(),
            ));
        }

        if self.audit.rotation_enabled && self.audit.retention_files == 0 {
            return Err(ConfigError::Config(
                "audit.retention_files must be greater than 0 when audit rotation is enabled"
                    .to_string(),
            ));
        }

        let now = OffsetDateTime::now_utc();
        if let Some(rule) = self
            .allowlist
            .iter()
            .find(|rule| rule.expires_at.is_some_and(|expires_at| expires_at <= now))
        {
            return Err(ConfigError::Config(format!(
                "allowlist rule '{}' is expired and cannot be used at runtime",
                rule.pattern
            )));
        }

        if let Some(rule) = self
            .blocklist
            .iter()
            .find(|rule| rule.expires_at.is_some_and(|expires_at| expires_at <= now))
        {
            return Err(ConfigError::Config(format!(
                "blocklist rule '{}' is expired and cannot be used at runtime",
                rule.pattern
            )));
        }

        rules::validate_trusted_aliases(&self.language_analysis.trusted_aliases)?;

        crate::validate::validate_policy_rules(&self.rules)
            .map_err(|(index, err)| ConfigError::Config(format!("rules[{index}]: {err}")))?;

        Ok(())
    }

    fn validate_runtime_requirements_for_path(&self, path: &Path) -> Result<()> {
        self.validate_runtime_requirements_selective()
            .map_err(|err| match err {
                ConfigError::Config(message) => {
                    ConfigError::Config(format!("invalid config {}: {message}", path.display()))
                }
                other => other,
            })
    }

    pub fn layered_allowlist_rules(&self) -> Vec<LayeredAllowlistRule> {
        self.allowlist
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, rule)| {
                let source_layer = self
                    .allowlist_layers
                    .get(index)
                    .copied()
                    .unwrap_or(ConfigSourceLayer::Project);

                LayeredAllowlistRule { rule, source_layer }
            })
            .collect()
    }

    pub fn layered_blocklist_rules(&self) -> Vec<LayeredBlocklistRule> {
        self.blocklist
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, rule)| {
                let source_layer = self
                    .blocklist_layers
                    .get(index)
                    .copied()
                    .unwrap_or(ConfigSourceLayer::Project);

                LayeredBlocklistRule { rule, source_layer }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
