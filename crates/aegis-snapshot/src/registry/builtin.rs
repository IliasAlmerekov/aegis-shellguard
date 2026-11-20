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

    /// Whether `auto_snapshot_*` config enables this provider under the selective policy.
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
