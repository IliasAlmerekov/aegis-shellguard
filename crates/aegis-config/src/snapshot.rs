//! Snapshot provider configuration types.

use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema,
)]
#[serde(rename_all = "PascalCase")]
#[schemars(description = "Which Docker containers to include in snapshots.

- `Labeled` (default) — only containers with a specific label.
- `All`               — every running container (legacy blanket behaviour).
- `Names`             — containers whose name matches one of the given patterns.")]
pub enum DockerScopeMode {
    #[schemars(description = "Only snapshot containers carrying the opt-in label (default).")]
    #[default]
    Labeled,
    #[schemars(description = "Snapshot every running container — use with care on busy hosts.")]
    All,
    #[schemars(description = "Snapshot containers whose name matches at least one pattern.")]
    Names,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
#[schemars(
    description = "Scoping rules that decide *which* Docker containers are eligible for snapshot.

Stored under `[docker_scope]` in `aegis.toml`."
)]
pub struct DockerScope {
    #[schemars(description = "Selection strategy.")]
    pub mode: DockerScopeMode,
    #[schemars(
        description = "Label selector for `Labeled` mode.  The container must carry this label
with value `\"true\"` to be eligible (e.g. `aegis.snapshot=true`)."
    )]
    pub label: String,
    #[schemars(
        description = "Name patterns for `Names` mode.  Each pattern is passed as a separate
`--filter name=<pat>` argument to `docker ps` (Docker ORs them)."
    )]
    pub name_patterns: Vec<String>,
}

impl Default for DockerScope {
    fn default() -> Self {
        Self {
            mode: DockerScopeMode::Labeled,
            label: "aegis.snapshot".to_string(),
            name_patterns: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
#[schemars(description = "Connection settings for a PostgreSQL snapshot plugin.

Credentials are provided externally via `PGPASSWORD` or `~/.pgpass`.
The plugin is a no-op when [`database`](Self::database) is empty.")]
pub struct PostgresSnapshotConfig {
    #[schemars(description = "PostgreSQL database name to snapshot.")]
    pub database: String,
    #[schemars(description = "PostgreSQL host name or address.")]
    pub host: String,
    #[schemars(description = "PostgreSQL port.")]
    pub port: u16,
    #[schemars(description = "PostgreSQL user name.")]
    pub user: String,
}

impl Default for PostgresSnapshotConfig {
    fn default() -> Self {
        Self {
            database: String::new(),
            host: "localhost".to_string(),
            port: 5432,
            user: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
#[schemars(
    description = "Configuration settings for the Supabase snapshot provider.

`project_ref` is advisory-only for audit/UI and future phases. The
rollback target-match setting is part of the bundle definition for later
rollback flows that compare the active config target with the manifest
target. Phase 1 uses the direct PostgreSQL transport in [`db`](Self::db)."
)]
pub struct SupabaseSnapshotConfig {
    #[schemars(
        description = "Advisory-only Supabase project reference for audit/UI and future phases."
    )]
    pub project_ref: String,
    #[schemars(
        description = "Target-match preference for rollback flows that compare config targets."
    )]
    pub require_config_target_match_on_rollback: bool,
    #[schemars(description = "Phase 1 direct PostgreSQL transport for Supabase snapshots.")]
    pub db: PostgresSnapshotConfig,
}

impl Default for SupabaseSnapshotConfig {
    fn default() -> Self {
        Self {
            project_ref: String::new(),
            require_config_target_match_on_rollback: true,
            db: PostgresSnapshotConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
#[schemars(description = "Connection settings for a MySQL snapshot plugin.

Credentials are provided externally via `MYSQL_PWD` or `~/.my.cnf`.
The plugin is a no-op when [`database`](Self::database) is empty.")]
pub struct MysqlSnapshotConfig {
    #[schemars(description = "MySQL database name to snapshot.")]
    pub database: String,
    #[schemars(description = "MySQL host name or address.")]
    pub host: String,
    #[schemars(description = "MySQL port.")]
    pub port: u16,
    #[schemars(description = "MySQL user name.")]
    pub user: String,
}

impl Default for MysqlSnapshotConfig {
    fn default() -> Self {
        Self {
            database: String::new(),
            host: "localhost".to_string(),
            port: 3306,
            user: String::new(),
        }
    }
}
