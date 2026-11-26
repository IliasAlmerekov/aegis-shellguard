use std::path::{Path, PathBuf};

use aegis_config::{
    AegisConfig, DockerScope, MysqlSnapshotConfig, PostgresSnapshotConfig, SupabaseSnapshotConfig,
};
use aegis_types::{RecordedSnapshot, SnapshotPolicy, SnapshotRecord};

mod builtin;
mod coverage;
pub use coverage::SnapshotCoverage;
use coverage::{COMMAND_READINESS_BUDGET, attempt};

use crate::SnapshotPlugin;
use crate::error::SnapshotError;
use crate::liveness::{LIVENESS_TIMEOUT, Liveness};
use crate::paths::resolve_snapshots_dir;
use crate::readiness::{ATTEMPT_TIMEOUT, ArtifactReadiness};
use crate::retention::{PrunableRecord, prunable_records_newest_first};

pub fn available_provider_names() -> &'static [&'static str] {
    &builtin::BUILTIN_SNAPSHOT_PROVIDER_NAMES
}

pub struct SnapshotRegistry {
    plugins: Vec<Box<dyn SnapshotPlugin>>,
}

#[derive(Debug, Clone)]
pub struct SnapshotRegistryConfig {
    pub snapshot_policy: SnapshotPolicy,
    pub auto_snapshot_git: bool,
    pub auto_snapshot_docker: bool,
    pub auto_snapshot_postgres: bool,
    pub postgres_snapshot: PostgresSnapshotConfig,
    pub auto_snapshot_mysql: bool,
    pub mysql_snapshot: MysqlSnapshotConfig,
    pub auto_snapshot_supabase: bool,
    pub supabase_snapshot: SupabaseSnapshotConfig,
    pub auto_snapshot_sqlite: bool,
    pub sqlite_snapshot_path: String,
    pub snapshots_dir: PathBuf,
    pub docker_scope: DockerScope,
}

fn registry_config_from_parts(
    config: &AegisConfig,
    snapshots_dir: PathBuf,
) -> SnapshotRegistryConfig {
    SnapshotRegistryConfig {
        snapshot_policy: config.snapshot_policy,
        auto_snapshot_git: config.auto_snapshot_git,
        auto_snapshot_docker: config.auto_snapshot_docker,
        auto_snapshot_postgres: config.auto_snapshot_postgres,
        postgres_snapshot: config.postgres_snapshot.clone(),
        auto_snapshot_mysql: config.auto_snapshot_mysql,
        mysql_snapshot: config.mysql_snapshot.clone(),
        auto_snapshot_supabase: config.auto_snapshot_supabase,
        supabase_snapshot: config.supabase_snapshot.clone(),
        auto_snapshot_sqlite: config.auto_snapshot_sqlite,
        sqlite_snapshot_path: config.sqlite_snapshot_path.clone(),
        snapshots_dir,
        docker_scope: config.docker_scope.clone(),
    }
}

async fn liveness_within(
    limit: std::time::Duration,
    plugin: &dyn SnapshotPlugin,
    snapshot_id: &str,
) -> std::result::Result<Liveness, SnapshotError> {
    tokio::time::timeout(limit, plugin.liveness(snapshot_id))
        .await
        .unwrap_or_else(|_| {
            Ok(Liveness::Inconclusive {
                reason: format!("liveness check did not finish within {limit:?}"),
            })
        })
}

impl SnapshotRegistryConfig {
    pub fn try_new(config: &AegisConfig) -> std::result::Result<Self, SnapshotError> {
        let snapshots_dir = resolve_snapshots_dir()?;
        Ok(registry_config_from_parts(config, snapshots_dir))
    }

    pub fn for_rollback_from_config(
        config: &AegisConfig,
    ) -> std::result::Result<Self, SnapshotError> {
        let mut runtime_config = Self::try_new(config)?;
        runtime_config.snapshot_policy = SnapshotPolicy::Full;
        runtime_config.auto_snapshot_git = true;
        runtime_config.auto_snapshot_docker = true;
        runtime_config.auto_snapshot_postgres = true;
        runtime_config.auto_snapshot_mysql = true;
        runtime_config.auto_snapshot_supabase = true;
        runtime_config.auto_snapshot_sqlite = true;
        Ok(runtime_config)
    }
}

impl SnapshotRegistry {
    #[cfg(any(test, feature = "testing"))]
    pub fn new_with_plugins(plugins: Vec<Box<dyn SnapshotPlugin>>) -> Self {
        Self { plugins }
    }

    pub fn from_runtime_config(config: &SnapshotRegistryConfig) -> Self {
        #[cfg(any(test, feature = "testing"))]
        crate::testing::increment_registry_build_count();

        let mut plugins: Vec<Box<dyn SnapshotPlugin>> = Vec::new();

        match config.snapshot_policy {
            SnapshotPolicy::None => {}
            SnapshotPolicy::Selective => {
                plugins = builtin::BuiltinProvider::ALL
                    .into_iter()
                    .filter(|provider| provider.auto_snapshot_enabled(config))
                    .map(|provider| provider.materialize(config))
                    .collect();
            }
            SnapshotPolicy::Full => {
                plugins = builtin::BuiltinProvider::ALL
                    .into_iter()
                    .map(|provider| provider.materialize(config))
                    .collect();
            }
        }

        Self { plugins }
    }

    pub fn for_rollback() -> std::result::Result<Self, SnapshotError> {
        Ok(Self::from_runtime_config(
            &SnapshotRegistryConfig::for_rollback_from_config(&AegisConfig::default())?,
        ))
    }

    pub fn configured_provider_names(&self) -> Vec<&'static str> {
        self.plugins.iter().map(|plugin| plugin.name()).collect()
    }

    pub async fn snapshot_all(&self, cwd: &Path, cmd: &str) -> SnapshotCoverage {
        self.snapshot_matching(cwd, cmd, |_| true).await
    }

    pub async fn snapshot_all_except(
        &self,
        excluded: &str,
        cwd: &Path,
        cmd: &str,
    ) -> SnapshotCoverage {
        self.snapshot_matching(cwd, cmd, |name| name != excluded)
            .await
    }

    pub async fn snapshot_plugin_at(
        &self,
        plugin: &str,
        dir: &Path,
        cmd: &str,
    ) -> SnapshotCoverage {
        self.snapshot_matching(dir, cmd, |name| name == plugin)
            .await
    }

    pub async fn plugin_applicable_at(&self, plugin: &str, dir: &Path) -> bool {
        for candidate in &self.plugins {
            if candidate.name() == plugin {
                return candidate.is_applicable(dir).await;
            }
        }
        false
    }

    async fn snapshot_matching(
        &self,
        cwd: &Path,
        cmd: &str,
        include: impl Fn(&str) -> bool,
    ) -> SnapshotCoverage {
        let mut coverage = SnapshotCoverage::default();
        for plugin in &self.plugins {
            if !include(plugin.name()) || !plugin.is_applicable(cwd).await {
                continue;
            }
            coverage.applicable += 1;
            match plugin.snapshot(cwd, cmd).await {
                Ok(snapshot_id) => coverage.records.push(SnapshotRecord {
                    plugin: plugin.name(),
                    snapshot_id,
                    git_target: plugin.snapshot_target(cwd).await,
                }),
                Err(e) => {
                    coverage.unrecorded.push(plugin.name());
                    tracing::warn!(
                        plugin = plugin.name(),
                        error = %e,
                        "{}",
                        crate::SNAPSHOT_FAILED_CONTINUING
                    );
                }
            }
        }
        coverage
    }

    pub async fn check_readiness(&self, coverage: &mut SnapshotCoverage) {
        let deadline = tokio::time::Instant::now() + COMMAND_READINESS_BUDGET;
        let mut attempts = Vec::with_capacity(coverage.records.len());
        for record in &coverage.records {
            let limit = ATTEMPT_TIMEOUT
                .min(deadline.saturating_duration_since(tokio::time::Instant::now()));
            let outcome = match self.plugins.iter().find(|p| p.name() == record.plugin) {
                Some(plugin) if !limit.is_zero() => {
                    tokio::time::timeout(limit, plugin.readiness(&record.snapshot_id))
                        .await
                        .unwrap_or(ArtifactReadiness::UNAVAILABLE)
                }
                _ => ArtifactReadiness::UNAVAILABLE,
            };
            attempts.push(attempt(record.plugin, outcome));
        }
        coverage.attempts = attempts;
    }

    pub async fn applicable_plugins(&self, cwd: &Path) -> Vec<&'static str> {
        let mut names = Vec::new();
        for plugin in &self.plugins {
            if plugin.is_applicable(cwd).await {
                names.push(plugin.name());
            }
        }
        names
    }

    pub async fn rollback(
        &self,
        plugin_name: &str,
        snapshot_id: &str,
    ) -> std::result::Result<(), SnapshotError> {
        let plugin = self
            .plugins
            .iter()
            .find(|plugin| plugin.name() == plugin_name)
            .ok_or_else(|| SnapshotError::PluginUnavailable {
                plugin: plugin_name.to_string(),
                operation: "rollback",
            })?;

        plugin.rollback(snapshot_id).await
    }

    pub async fn delete(
        &self,
        plugin_name: &str,
        snapshot_id: &str,
    ) -> std::result::Result<(), SnapshotError> {
        let plugin = self
            .plugins
            .iter()
            .find(|plugin| plugin.name() == plugin_name)
            .ok_or_else(|| SnapshotError::PluginUnavailable {
                plugin: plugin_name.to_string(),
                operation: "delete",
            })?;

        plugin.delete(snapshot_id).await
    }

    pub async fn liveness(
        &self,
        plugin_name: &str,
        snapshot_id: &str,
    ) -> std::result::Result<Liveness, SnapshotError> {
        let plugin = self
            .plugins
            .iter()
            .find(|plugin| plugin.name() == plugin_name)
            .ok_or_else(|| SnapshotError::PluginUnavailable {
                plugin: plugin_name.to_string(),
                operation: "rollback",
            })?;

        liveness_within(LIVENESS_TIMEOUT, plugin.as_ref(), snapshot_id).await
    }

    pub fn resolve_prunable_records(&self, records: &[RecordedSnapshot]) -> Vec<PrunableRecord> {
        prunable_records_newest_first(records)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use tracing::field::{Field, Visit};
    use tracing::span;

    use super::*;

    struct AlwaysFailingPlugin;

    #[async_trait]
    impl SnapshotPlugin for AlwaysFailingPlugin {
        fn name(&self) -> &'static str {
            "mock-failing"
        }

        async fn is_applicable(&self, _cwd: &Path) -> bool {
            true
        }

        async fn snapshot(
            &self,
            _cwd: &Path,
            _cmd: &str,
        ) -> std::result::Result<String, SnapshotError> {
            Err(SnapshotError::Snapshot("mock plugin failure".to_string()))
        }

        async fn rollback(&self, _snapshot_id: &str) -> std::result::Result<(), SnapshotError> {
            Ok(())
        }

        async fn delete(&self, _snapshot_id: &str) -> std::result::Result<(), SnapshotError> {
            Ok(())
        }

        async fn liveness(
            &self,
            _snapshot_id: &str,
        ) -> std::result::Result<Liveness, SnapshotError> {
            Ok(Liveness::Present)
        }

        async fn readiness(&self, _snapshot_id: &str) -> ArtifactReadiness {
            ArtifactReadiness::UNAVAILABLE
        }
    }

    #[derive(Default)]
    struct CapturedFields {
        message: Option<String>,
        plugin: Option<String>,
        error: Option<String>,
    }

    #[derive(Default)]
    struct FieldVisitor(CapturedFields);

    impl Visit for FieldVisitor {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            let rendered = format!("{value:?}");
            match field.name() {
                "message" => self.0.message = Some(rendered),
                "plugin" => self.0.plugin = Some(rendered),
                "error" => self.0.error = Some(rendered),
                _ => {}
            }
        }

        fn record_str(&mut self, field: &Field, value: &str) {
            match field.name() {
                "message" => self.0.message = Some(value.to_string()),
                "plugin" => self.0.plugin = Some(value.to_string()),
                "error" => self.0.error = Some(value.to_string()),
                _ => {}
            }
        }
    }

    struct CollectingSubscriber(Arc<Mutex<Option<CapturedFields>>>);

    impl tracing::Subscriber for CollectingSubscriber {
        fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
            true
        }

        fn new_span(&self, _span: &span::Attributes<'_>) -> span::Id {
            span::Id::from_u64(1)
        }

        fn record(&self, _span: &span::Id, _values: &span::Record<'_>) {}

        fn record_follows_from(&self, _span: &span::Id, _follows: &span::Id) {}

        fn event(&self, event: &tracing::Event<'_>) {
            let mut visitor = FieldVisitor::default();
            event.record(&mut visitor);
            *self.0.lock().unwrap() = Some(visitor.0);
        }

        fn enter(&self, _span: &span::Id) {}

        fn exit(&self, _span: &span::Id) {}
    }

    struct AlwaysSucceedingPlugin;

    #[async_trait]
    impl SnapshotPlugin for AlwaysSucceedingPlugin {
        fn name(&self) -> &'static str {
            "mock-succeeding"
        }

        async fn is_applicable(&self, _cwd: &Path) -> bool {
            true
        }

        async fn snapshot(
            &self,
            _cwd: &Path,
            _cmd: &str,
        ) -> std::result::Result<String, SnapshotError> {
            Ok("mock-snapshot-id".to_string())
        }

        async fn rollback(&self, _snapshot_id: &str) -> std::result::Result<(), SnapshotError> {
            Ok(())
        }

        async fn delete(&self, _snapshot_id: &str) -> std::result::Result<(), SnapshotError> {
            Ok(())
        }

        async fn readiness(&self, _snapshot_id: &str) -> ArtifactReadiness {
            ArtifactReadiness::PRESENT_UNCHECKED
        }

        async fn liveness(
            &self,
            _snapshot_id: &str,
        ) -> std::result::Result<Liveness, SnapshotError> {
            Ok(Liveness::Present)
        }
    }

    struct NeverApplicablePlugin;

    #[async_trait]
    impl SnapshotPlugin for NeverApplicablePlugin {
        fn name(&self) -> &'static str {
            "mock-inapplicable"
        }

        async fn is_applicable(&self, _cwd: &Path) -> bool {
            false
        }

        async fn snapshot(
            &self,
            _cwd: &Path,
            _cmd: &str,
        ) -> std::result::Result<String, SnapshotError> {
            panic!("an inapplicable plugin must never be asked for a snapshot")
        }

        async fn rollback(&self, _snapshot_id: &str) -> std::result::Result<(), SnapshotError> {
            Ok(())
        }

        async fn delete(&self, _snapshot_id: &str) -> std::result::Result<(), SnapshotError> {
            Ok(())
        }

        async fn readiness(&self, _snapshot_id: &str) -> ArtifactReadiness {
            ArtifactReadiness::PRESENT_UNCHECKED
        }

        async fn liveness(
            &self,
            _snapshot_id: &str,
        ) -> std::result::Result<Liveness, SnapshotError> {
            Ok(Liveness::Present)
        }
    }

    mod liveness_tests;
    mod plugin_lookup_tests;
    mod readiness_tests;
    mod targeted_tests;

    #[test]
    fn snapshot_all_counts_a_failed_applicable_plugin_as_partial_coverage() {
        let registry = SnapshotRegistry::new_with_plugins(vec![
            Box::new(AlwaysSucceedingPlugin),
            Box::new(AlwaysFailingPlugin),
        ]);
        let runtime = tokio::runtime::Runtime::new().unwrap();

        let coverage =
            runtime.block_on(registry.snapshot_all(Path::new("/nonexistent-cwd"), "rm -rf /"));

        assert_eq!(coverage.records.len(), 1);
        assert_eq!(coverage.applicable, 2);
        assert!(coverage.is_partial());
    }

    #[test]
    fn snapshot_all_leaves_inapplicable_plugins_out_of_the_applicable_count() {
        let registry = SnapshotRegistry::new_with_plugins(vec![
            Box::new(AlwaysSucceedingPlugin),
            Box::new(NeverApplicablePlugin),
        ]);
        let runtime = tokio::runtime::Runtime::new().unwrap();

        let coverage =
            runtime.block_on(registry.snapshot_all(Path::new("/nonexistent-cwd"), "rm -rf /"));

        assert_eq!(coverage.applicable, 1);
        assert!(
            !coverage.is_partial(),
            "a plugin that has nothing to do here must not read as missing coverage"
        );
    }

    #[test]
    fn merge_adds_applicable_counts_and_appends_records() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let registry = SnapshotRegistry::new_with_plugins(vec![Box::new(AlwaysSucceedingPlugin)]);
        let mut first = runtime.block_on(registry.snapshot_all(Path::new("/x"), "c"));
        first.applicable = 2;
        let mut second = runtime.block_on(registry.snapshot_all(Path::new("/x"), "c"));
        second.applicable = 3;

        first.merge(second);

        assert_eq!(first.applicable, 5);
        assert_eq!(first.records.len(), 2);
    }

    #[test]
    fn snapshot_all_except_skips_only_the_excluded_plugin() {
        let registry = SnapshotRegistry::new_with_plugins(vec![
            Box::new(AlwaysSucceedingPlugin),
            Box::new(AlwaysFailingPlugin),
        ]);
        let runtime = tokio::runtime::Runtime::new().unwrap();

        let coverage = runtime.block_on(registry.snapshot_all_except(
            "mock-failing",
            Path::new("/nonexistent-cwd"),
            "rm -rf /",
        ));

        assert_eq!(coverage.applicable, 1);
        assert_eq!(coverage.records.len(), 1);
    }

    #[test]
    fn snapshot_plugin_at_runs_only_the_named_plugin() {
        let registry = SnapshotRegistry::new_with_plugins(vec![
            Box::new(AlwaysSucceedingPlugin),
            Box::new(AlwaysFailingPlugin),
        ]);
        let runtime = tokio::runtime::Runtime::new().unwrap();

        let coverage = runtime.block_on(registry.snapshot_plugin_at(
            "mock-succeeding",
            Path::new("/nonexistent-cwd"),
            "rm -rf /",
        ));

        assert_eq!(coverage.applicable, 1);
        assert_eq!(coverage.records.len(), 1);
    }

    #[test]
    fn plugin_applicable_at_is_true_for_a_named_applicable_plugin() {
        let registry = SnapshotRegistry::new_with_plugins(vec![
            Box::new(NeverApplicablePlugin),
            Box::new(AlwaysSucceedingPlugin),
        ]);
        let runtime = tokio::runtime::Runtime::new().unwrap();

        assert!(
            runtime.block_on(registry.plugin_applicable_at("mock-succeeding", Path::new("/x")))
        );
    }

    #[test]
    fn plugin_applicable_at_is_false_for_an_inapplicable_or_missing_plugin() {
        let registry = SnapshotRegistry::new_with_plugins(vec![
            Box::new(NeverApplicablePlugin),
            Box::new(AlwaysSucceedingPlugin),
        ]);
        let runtime = tokio::runtime::Runtime::new().unwrap();

        assert!(
            !runtime.block_on(registry.plugin_applicable_at("mock-inapplicable", Path::new("/x")))
        );
        assert!(!runtime.block_on(registry.plugin_applicable_at("absent", Path::new("/x"))));
    }

    #[test]
    fn snapshot_all_warns_with_plugin_and_error_fields_on_plugin_failure() {
        let registry = SnapshotRegistry::new_with_plugins(vec![Box::new(AlwaysFailingPlugin)]);
        let captured: Arc<Mutex<Option<CapturedFields>>> = Arc::new(Mutex::new(None));
        let subscriber = CollectingSubscriber(Arc::clone(&captured));

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let records = tracing::subscriber::with_default(subscriber, || {
            runtime.block_on(registry.snapshot_all(Path::new("/nonexistent-cwd"), "rm -rf /"))
        });

        assert!(
            records.records.is_empty(),
            "a failing plugin must not produce a snapshot record"
        );

        let captured =
            captured.lock().unwrap().take().expect(
                "snapshot_all must emit exactly one tracing event when the only plugin fails",
            );
        assert_eq!(
            captured.message.as_deref(),
            Some(crate::SNAPSHOT_FAILED_CONTINUING)
        );
        assert_eq!(captured.plugin.as_deref(), Some("mock-failing"));
        let error = captured.error.expect("error field must be recorded");
        assert!(
            error.contains("mock plugin failure"),
            "error field must carry the plugin's failure detail, got {error:?}"
        );
        assert!(!error.contains("rm -rf /"));
        assert_ne!(captured.message.as_deref(), Some("rm -rf /"));
    }
}
