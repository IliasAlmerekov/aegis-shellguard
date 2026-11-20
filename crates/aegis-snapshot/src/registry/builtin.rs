//! The built-in snapshot providers, named by a type so every `match` over them is exhaustive.

use std::path::PathBuf;

use crate::{
    DockerPlugin, GitPlugin, MysqlPlugin, PostgresPlugin, SnapshotPlugin, SqlitePlugin,
    SupabasePlugin,
};

use super::SnapshotRegistryConfig;

/// The snapshot providers built into this binary, in registry order.
#[derive(Clone, Copy)]
pub(super) enum BuiltinProvider {
    Git,
    Docker,
    Postgres,
    Mysql,
    Sqlite,
    Supabase,
}

impl BuiltinProvider {
    pub(super) const ALL: [Self; 6] = [
        Self::Git,
        Self::Docker,
        Self::Postgres,
        Self::Mysql,
        Self::Sqlite,
        Self::Supabase,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::Git => "git",
            Self::Docker => "docker",
            Self::Postgres => "postgres",
            Self::Mysql => "mysql",
            Self::Sqlite => "sqlite",
            Self::Supabase => "supabase",
        }
    }

    pub(super) fn auto_snapshot_enabled(self, config: &SnapshotRegistryConfig) -> bool {
        match self {
            Self::Git => config.auto_snapshot_git,
            Self::Docker => config.auto_snapshot_docker,
            Self::Postgres => config.auto_snapshot_postgres,
            Self::Mysql => config.auto_snapshot_mysql,
            Self::Sqlite => config.auto_snapshot_sqlite,
            Self::Supabase => config.auto_snapshot_supabase,
        }
    }

    pub(super) fn materialize(self, config: &SnapshotRegistryConfig) -> Box<dyn SnapshotPlugin> {
        match self {
            Self::Git => Box::new(GitPlugin),
            Self::Docker => Box::new(DockerPlugin::new().with_scope(config.docker_scope.clone())),
            Self::Postgres => Box::new(PostgresPlugin::new(
                config.postgres_snapshot.database.clone(),
                config.postgres_snapshot.host.clone(),
                config.postgres_snapshot.port,
                config.postgres_snapshot.user.clone(),
                config.snapshots_dir.clone(),
            )),
            Self::Mysql => Box::new(MysqlPlugin::new(
                config.mysql_snapshot.database.clone(),
                config.mysql_snapshot.host.clone(),
                config.mysql_snapshot.port,
                config.mysql_snapshot.user.clone(),
                config.snapshots_dir.clone(),
            )),
            Self::Sqlite => Box::new(SqlitePlugin::new(
                PathBuf::from(&config.sqlite_snapshot_path),
                config.snapshots_dir.clone(),
            )),
            Self::Supabase => Box::new(SupabasePlugin::new(
                config.supabase_snapshot.clone(),
                config.snapshots_dir.clone(),
            )),
        }
    }
}

pub(super) const BUILTIN_SNAPSHOT_PROVIDER_NAMES: [&str; BuiltinProvider::ALL.len()] = {
    let mut names = [""; BuiltinProvider::ALL.len()];
    let mut i = 0;
    while i < names.len() {
        names[i] = BuiltinProvider::ALL[i].name();
        i += 1;
    }
    names
};

#[cfg(test)]
mod tests {
    use aegis_config::AegisConfig;
    use aegis_types::SnapshotPolicy;

    use super::super::{SnapshotRegistry, registry_config_from_parts};
    use super::BuiltinProvider;

    #[test]
    fn selective_policy_materializes_exactly_the_enabled_provider() {
        for provider in BuiltinProvider::ALL {
            let mut config = AegisConfig::default();
            config.snapshot_policy = SnapshotPolicy::Selective;
            config.auto_snapshot_git = false;
            config.auto_snapshot_docker = false;
            config.auto_snapshot_postgres = false;
            config.auto_snapshot_mysql = false;
            config.auto_snapshot_sqlite = false;
            config.auto_snapshot_supabase = false;
            match provider {
                BuiltinProvider::Git => config.auto_snapshot_git = true,
                BuiltinProvider::Docker => config.auto_snapshot_docker = true,
                BuiltinProvider::Postgres => config.auto_snapshot_postgres = true,
                BuiltinProvider::Mysql => config.auto_snapshot_mysql = true,
                BuiltinProvider::Sqlite => config.auto_snapshot_sqlite = true,
                BuiltinProvider::Supabase => config.auto_snapshot_supabase = true,
            }

            let registry = SnapshotRegistry::from_runtime_config(&registry_config_from_parts(
                &config,
                std::env::temp_dir(),
            ));

            assert_eq!(registry.configured_provider_names(), [provider.name()]);
        }
    }
}
