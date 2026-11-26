#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable
    )
)]

use std::path::Path;

use async_trait::async_trait;

pub mod error;
pub use error::SnapshotError;

mod binary_lookup;
mod busy;
mod database_target;
mod docker;
mod git;
mod mysql;
mod postgres;
mod sqlite;
mod supabase;

pub use docker::DockerPlugin;
pub use git::{GitPlugin, git_toplevel};
pub use mysql::MysqlPlugin;
pub use postgres::PostgresPlugin;
pub use sqlite::SqlitePlugin;
pub use supabase::SupabasePlugin;

mod clock;
mod containment;
mod liveness;
mod paths;
mod readiness;
mod registry;
mod retention;
mod secure_fs;
mod snapshot_id;
#[cfg(test)]
mod test_support;
#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use clock::{Clock, FixedClock, SystemClock};
pub use liveness::Liveness;
pub use readiness::ArtifactReadiness;
pub use registry::{
    SnapshotCoverage, SnapshotRegistry, SnapshotRegistryConfig, available_provider_names,
};
pub use retention::{PrunableRecord, RetentionPolicy};

type Result<T> = std::result::Result<T, SnapshotError>;

pub const SNAPSHOT_FAILED_CONTINUING: &str = "snapshot failed, continuing";

#[async_trait]
pub trait SnapshotPlugin: Send + Sync {
    fn name(&self) -> &'static str;

    async fn is_applicable(&self, cwd: &Path) -> bool;

    async fn snapshot(&self, cwd: &Path, cmd: &str) -> Result<String>;

    async fn snapshot_target(&self, _cwd: &Path) -> Option<String> {
        None
    }

    async fn rollback(&self, snapshot_id: &str) -> Result<()>;

    async fn delete(&self, snapshot_id: &str) -> Result<()>;

    async fn liveness(&self, snapshot_id: &str) -> Result<Liveness>;

    async fn readiness(&self, snapshot_id: &str) -> ArtifactReadiness;
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use aegis_config::SupabaseSnapshotConfig;

    use super::*;

    #[tokio::test]
    async fn test_git_plugin_delete_missing_artifact_is_idempotent() {
        let result = GitPlugin
            .delete("/tmp/aegis-missing-git-repo-test\tdeadbeef")
            .await;
        assert!(
            result.is_ok(),
            "delete on a missing git snapshot must be idempotent: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_docker_plugin_delete_none_sentinel_is_noop() {
        let plugin = DockerPlugin::new();
        let result = plugin.delete("none").await;
        assert!(
            result.is_ok(),
            "delete on 'none' sentinel must succeed: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_postgres_plugin_delete_missing_dump_returns_ok() {
        let temp = tempfile::tempdir().unwrap();
        let missing_dump = temp.path().join("missing.dump");
        let plugin = PostgresPlugin::new(
            "db".to_string(),
            "localhost".to_string(),
            5432,
            "user".to_string(),
            temp.path().to_path_buf(),
        );
        let result = plugin
            .delete(&format!(
                "v2\t6462\t6c6f63616c686f7374\t5432\t75736572\t{}",
                crate::snapshot_id::encode_path(&missing_dump)
            ))
            .await;
        assert!(
            result.is_ok(),
            "delete on missing postgres dump must be idempotent: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_mysql_plugin_delete_missing_dump_returns_ok() {
        let temp = tempfile::tempdir().unwrap();
        let missing_dump = temp.path().join("missing.sql");
        let plugin = MysqlPlugin::new(
            "db".to_string(),
            "localhost".to_string(),
            3306,
            "user".to_string(),
            temp.path().to_path_buf(),
        );
        let result = plugin
            .delete(&format!(
                "v2\t6462\t6c6f63616c686f7374\t3306\t75736572\t{}",
                crate::snapshot_id::encode_path(&missing_dump)
            ))
            .await;
        assert!(
            result.is_ok(),
            "delete on missing mysql dump must be idempotent: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_sqlite_plugin_delete_missing_dump_returns_ok() {
        let temp = tempfile::tempdir().unwrap();
        let missing_dump = temp.path().join("missing.db");
        let plugin = SqlitePlugin::new(PathBuf::from("/tmp/app.db"), temp.path().to_path_buf());
        let result = plugin
            .delete(&format!(
                "v2\t2f746d702f6170702e6462\t{}",
                crate::snapshot_id::encode_path(&missing_dump)
            ))
            .await;
        assert!(
            result.is_ok(),
            "delete on missing sqlite dump must be idempotent: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_supabase_plugin_delete_missing_manifest_returns_ok() {
        let temp = tempfile::tempdir().unwrap();
        let plugin =
            SupabasePlugin::new(SupabaseSnapshotConfig::default(), temp.path().to_path_buf());
        let result = plugin
            .delete("supabase-v1\t2f6e6f6e652f6d616e69666573742e6a736f6e")
            .await;
        assert!(
            result.is_ok(),
            "delete on missing supabase manifest must be idempotent: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_registry_delete_dispatches_to_named_plugin() {
        let registry = SnapshotRegistry::new_with_plugins(vec![Box::new(GitPlugin)]);
        let result = registry.delete("git", "malformed-id").await;
        assert!(
            result.is_err(),
            "delete on malformed id must return an error: {result:?}"
        );
    }

    #[test]
    fn test_registry_resolve_prunable_records_lists_newest_first() {
        use std::time::{Duration, SystemTime};

        let created = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let ledger_records = vec![
            aegis_types::RecordedSnapshot {
                plugin: "git".to_string(),
                snapshot_id: "older".to_string(),
                recorded_at: created,
            },
            aegis_types::RecordedSnapshot {
                plugin: "docker".to_string(),
                snapshot_id: "newer".to_string(),
                recorded_at: created,
            },
        ];
        let registry = SnapshotRegistry::for_rollback().unwrap();

        let records = registry.resolve_prunable_records(&ledger_records);

        assert_eq!(
            records,
            vec![
                PrunableRecord {
                    plugin: "docker".to_string(),
                    snapshot_id: "newer".to_string(),
                    recorded_at: time::OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
                },
                PrunableRecord {
                    plugin: "git".to_string(),
                    snapshot_id: "older".to_string(),
                    recorded_at: time::OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
                },
            ]
        );
    }

    #[test]
    fn test_retention_policy_age_only_keeps_recent() {
        use time::OffsetDateTime;

        let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let policy = RetentionPolicy::from_max_age_days(7);
        let records = vec![
            PrunableRecord {
                plugin: "git".to_string(),
                snapshot_id: "old".to_string(),
                recorded_at: now - time::Duration::days(10),
            },
            PrunableRecord {
                plugin: "git".to_string(),
                snapshot_id: "recent".to_string(),
                recorded_at: now - time::Duration::days(2),
            },
        ];
        let candidates = policy.apply(&records, now);
        let ids: Vec<_> = candidates.iter().map(|r| r.snapshot_id.as_str()).collect();
        assert_eq!(ids, vec!["old"]);
    }

    #[test]
    fn test_retention_policy_count_only_keeps_newest_n_per_provider() {
        use time::OffsetDateTime;

        let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let policy = RetentionPolicy::from_max_count_per_provider(2);
        let records = vec![
            PrunableRecord {
                plugin: "git".to_string(),
                snapshot_id: "git-3".to_string(),
                recorded_at: now - time::Duration::days(1),
            },
            PrunableRecord {
                plugin: "git".to_string(),
                snapshot_id: "git-2".to_string(),
                recorded_at: now - time::Duration::days(2),
            },
            PrunableRecord {
                plugin: "git".to_string(),
                snapshot_id: "git-1".to_string(),
                recorded_at: now - time::Duration::days(3),
            },
            PrunableRecord {
                plugin: "docker".to_string(),
                snapshot_id: "docker-1".to_string(),
                recorded_at: now - time::Duration::days(1),
            },
        ];
        let candidates = policy.apply(&records, now);
        let ids: Vec<_> = candidates.iter().map(|r| r.snapshot_id.as_str()).collect();
        assert_eq!(ids, vec!["git-1"]);
    }

    #[test]
    fn test_retention_policy_count_follows_ledger_order_not_timestamps() {
        use time::OffsetDateTime;

        let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let policy = RetentionPolicy::from_max_count_per_provider(1);
        let records = vec![
            PrunableRecord {
                plugin: "git".to_string(),
                snapshot_id: "newest-in-log".to_string(),
                recorded_at: now - time::Duration::days(5),
            },
            PrunableRecord {
                plugin: "git".to_string(),
                snapshot_id: "older-in-log".to_string(),
                recorded_at: now - time::Duration::days(1),
            },
        ];
        let candidates = policy.apply(&records, now);
        let ids: Vec<_> = candidates.iter().map(|r| r.snapshot_id.as_str()).collect();
        assert_eq!(ids, vec!["older-in-log"]);
    }

    #[test]
    fn test_retention_policy_union_keeps_any_record_that_passes_either_rule() {
        use time::OffsetDateTime;

        let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let policy = RetentionPolicy::from_config(&aegis_config::PruneConfig {
            enabled: true,
            max_count_per_provider: Some(1),
            max_age_days: Some(7),
        });
        let records = vec![
            PrunableRecord {
                plugin: "git".to_string(),
                snapshot_id: "count-kept".to_string(),
                recorded_at: now - time::Duration::days(1),
            },
            PrunableRecord {
                plugin: "git".to_string(),
                snapshot_id: "age-kept".to_string(),
                recorded_at: now - time::Duration::days(2),
            },
            PrunableRecord {
                plugin: "git".to_string(),
                snapshot_id: "prune-me".to_string(),
                recorded_at: now - time::Duration::days(10),
            },
        ];
        let candidates = policy.apply(&records, now);
        let ids: Vec<_> = candidates.iter().map(|r| r.snapshot_id.as_str()).collect();
        assert_eq!(ids, vec!["prune-me"]);
    }

    #[test]
    fn test_retention_policy_empty_input_yields_empty_candidates() {
        use time::OffsetDateTime;

        let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let policy = RetentionPolicy::from_max_count_per_provider(5);
        let candidates = policy.apply(&[], now);
        assert!(
            candidates.is_empty(),
            "empty input must produce no prune candidates"
        );
    }

    #[test]
    fn test_retention_policy_no_active_rules_yields_empty_candidates() {
        use time::OffsetDateTime;

        let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let policy = RetentionPolicy::default();
        let records = vec![
            PrunableRecord {
                plugin: "git".to_string(),
                snapshot_id: "old".to_string(),
                recorded_at: now - time::Duration::days(30),
            },
            PrunableRecord {
                plugin: "git".to_string(),
                snapshot_id: "new".to_string(),
                recorded_at: now - time::Duration::days(1),
            },
        ];
        let candidates = policy.apply(&records, now);
        assert!(
            candidates.is_empty(),
            "no active rules must produce no prune candidates"
        );
    }
}
