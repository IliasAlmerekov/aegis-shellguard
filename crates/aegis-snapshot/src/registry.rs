//! Snapshot registry materialization and runtime dispatch.

use std::path::{Path, PathBuf};

use aegis_config::{
    AegisConfig, DockerScope, MysqlSnapshotConfig, PostgresSnapshotConfig, SnapshotPolicy,
    SupabaseSnapshotConfig,
};
use aegis_types::SnapshotRecord;

use crate::error::SnapshotError;
use crate::paths::resolve_snapshots_dir;
use crate::retention::{PrunableRecord, resolve_prunable_records_from_default_audit_log};
use crate::testing::increment_registry_build_count;
use crate::{
    DockerPlugin, GitPlugin, MysqlPlugin, PostgresPlugin, SnapshotPlugin, SqlitePlugin,
    SupabasePlugin,
};

const BUILTIN_SNAPSHOT_PROVIDER_NAMES: &[&str] =
    &["git", "docker", "postgres", "mysql", "sqlite", "supabase"];

/// Return the names of snapshot providers built into this binary/runtime.
pub fn available_provider_names() -> &'static [&'static str] {
    BUILTIN_SNAPSHOT_PROVIDER_NAMES
}

fn materialize_builtin_plugin(
    name: &str,
    config: &SnapshotRegistryConfig,
) -> Box<dyn SnapshotPlugin> {
    match name {
        "git" => Box::new(GitPlugin),
        "docker" => Box::new(DockerPlugin::new().with_scope(config.docker_scope.clone())),
        "postgres" => Box::new(PostgresPlugin::new(
            config.postgres_snapshot.database.clone(),
            config.postgres_snapshot.host.clone(),
            config.postgres_snapshot.port,
            config.postgres_snapshot.user.clone(),
            config.snapshots_dir.clone(),
        )),
        "mysql" => Box::new(MysqlPlugin::new(
            config.mysql_snapshot.database.clone(),
            config.mysql_snapshot.host.clone(),
            config.mysql_snapshot.port,
            config.mysql_snapshot.user.clone(),
            config.snapshots_dir.clone(),
        )),
        "sqlite" => Box::new(SqlitePlugin::new(
            PathBuf::from(&config.sqlite_snapshot_path),
            config.snapshots_dir.clone(),
        )),
        "supabase" => Box::new(SupabasePlugin::new(
            config.supabase_snapshot.clone(),
            config.snapshots_dir.clone(),
        )),
        _ => panic!("unknown built-in snapshot provider {name:?}"),
    }
}

/// Materialize a set of built-in plugins by name.
///
/// Panics if any name is not a known built-in provider.
fn materialize_builtin_plugins(
    names: &[&str],
    config: &SnapshotRegistryConfig,
) -> Vec<Box<dyn SnapshotPlugin>> {
    names
        .iter()
        .map(|name| materialize_builtin_plugin(name, config))
        .collect()
}

/// Holds the runtime snapshot provider set used for snapshot and rollback flows.
///
/// Entries may be materialized from the effective runtime config or assembled
/// for broader recovery operations such as rollback. A provider being present
/// here means it is available for later applicability checks, not that it will
/// snapshot every command or in every working directory.
pub struct SnapshotRegistry {
    plugins: Vec<Box<dyn SnapshotPlugin>>,
}

/// What one [`SnapshotRegistry::snapshot_all`] pass produced and how much of
/// the applicable provider set it covers.
///
/// `records` holds the snapshots that were created. `applicable` counts the
/// plugins that reported themselves applicable during the same pass, whether
/// or not their attempt succeeded. `records.len() < applicable` means at least
/// one applicable plugin failed and the coverage is partial. Both numbers come
/// from one pass on purpose: sampling applicability again after the attempts
/// can return a different set, and a recovery decision must not rest on two
/// disagreeing samples.
#[derive(Debug, Default, Clone)]
pub struct SnapshotCoverage {
    /// The snapshots that were created, one per successful plugin attempt.
    pub records: Vec<SnapshotRecord>,
    /// How many plugins reported themselves applicable during the pass.
    pub applicable: usize,
}

impl SnapshotCoverage {
    /// Build a coverage result from records alone, treating every record as a
    /// successful attempt by an applicable plugin.
    ///
    /// For callers that never ran a registry pass, such as a code path that
    /// short-circuits before snapshot creation.
    #[must_use]
    pub fn complete(records: Vec<SnapshotRecord>) -> Self {
        Self {
            applicable: records.len(),
            records,
        }
    }

    /// Returns `true` when an applicable plugin failed while another succeeded.
    #[must_use]
    pub fn is_partial(&self) -> bool {
        !self.records.is_empty() && self.records.len() < self.applicable
    }
}

/// Eager runtime snapshot config used to materialize a [`SnapshotRegistry`].
///
/// This captures the config boundary between "which built-in providers are
/// available at runtime" and the later per-command/per-directory applicability
/// checks performed by each provider.
#[derive(Debug, Clone)]
pub struct SnapshotRegistryConfig {
    /// Global snapshot policy (None, Selective, or Full).
    pub snapshot_policy: SnapshotPolicy,
    /// Enable Git snapshots when true.
    pub auto_snapshot_git: bool,
    /// Enable Docker snapshots when true.
    pub auto_snapshot_docker: bool,
    /// Enable PostgreSQL snapshots when true.
    pub auto_snapshot_postgres: bool,
    /// Connection details for PostgreSQL snapshots.
    pub postgres_snapshot: PostgresSnapshotConfig,
    /// Enable MySQL snapshots when true.
    pub auto_snapshot_mysql: bool,
    /// Connection details for MySQL snapshots.
    pub mysql_snapshot: MysqlSnapshotConfig,
    /// Enable Supabase snapshots when true.
    pub auto_snapshot_supabase: bool,
    /// Connection details for Supabase snapshots.
    pub supabase_snapshot: SupabaseSnapshotConfig,
    /// Enable SQLite snapshots when true.
    pub auto_snapshot_sqlite: bool,
    /// Path to the SQLite database file to snapshot.
    pub sqlite_snapshot_path: String,
    /// Directory where snapshot artifacts are stored.
    pub snapshots_dir: PathBuf,
    /// Docker snapshot scope (image vs container).
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

impl SnapshotRegistryConfig {
    /// Fallible constructor — propagates `HOME`-unset error.
    pub fn try_new(config: &AegisConfig) -> std::result::Result<Self, SnapshotError> {
        let snapshots_dir = resolve_snapshots_dir()?;
        Ok(registry_config_from_parts(config, snapshots_dir))
    }

    /// Build a rollback runtime config that preserves effective provider
    /// settings while forcing all built-in providers to be available.
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
    /// Construct a registry from an explicit plugin list.
    ///
    /// This constructor is intended for testing only.  Production code should
    /// use [`SnapshotRegistry::from_runtime_config`] instead.
    pub fn new_with_plugins(plugins: Vec<Box<dyn SnapshotPlugin>>) -> Self {
        Self { plugins }
    }

    /// Fallible constructor that honours the effective runtime config.
    pub fn try_from_config(config: &AegisConfig) -> std::result::Result<Self, SnapshotError> {
        Ok(Self::from_runtime_config(&SnapshotRegistryConfig::try_new(
            config,
        )?))
    }

    /// Build a snapshot registry from the eager runtime config.
    ///
    /// This materializes the config-filtered set of available snapshot
    /// providers. Applicability remains a later concern evaluated by each
    /// provider for a specific working directory or command.
    pub fn from_runtime_config(config: &SnapshotRegistryConfig) -> Self {
        increment_registry_build_count();

        let mut plugins: Vec<Box<dyn SnapshotPlugin>> = Vec::new();

        match config.snapshot_policy {
            SnapshotPolicy::None => { /* no plugins */ }
            SnapshotPolicy::Selective => {
                let enabled_names: Vec<_> = available_provider_names()
                    .iter()
                    .copied()
                    .filter(|name| match *name {
                        "git" => config.auto_snapshot_git,
                        "docker" => config.auto_snapshot_docker,
                        "postgres" => config.auto_snapshot_postgres,
                        "mysql" => config.auto_snapshot_mysql,
                        "supabase" => config.auto_snapshot_supabase,
                        "sqlite" => config.auto_snapshot_sqlite,
                        _ => false,
                    })
                    .collect();
                plugins = materialize_builtin_plugins(&enabled_names, config);
            }
            SnapshotPolicy::Full => {
                plugins = materialize_builtin_plugins(available_provider_names(), config);
            }
        }

        Self { plugins }
    }

    /// Build a registry that can roll back any built-in snapshot type.
    ///
    /// This intentionally ignores per-plugin snapshot flags: operators must be
    /// able to restore previously recorded snapshots even if snapshot creation
    /// is disabled in the current config.
    pub fn for_rollback() -> std::result::Result<Self, SnapshotError> {
        Ok(Self::from_runtime_config(
            &SnapshotRegistryConfig::for_rollback_from_config(&AegisConfig::default())?,
        ))
    }

    /// Return the names of providers materialized into this registry instance.
    ///
    /// For registries built from runtime config, this reports the
    /// config-filtered materialized provider set. For registries built for
    /// other purposes, such as [`SnapshotRegistry::for_rollback`], it reports
    /// the providers materialized for that registry's use.
    pub fn configured_provider_names(&self) -> Vec<&'static str> {
        self.plugins.iter().map(|plugin| plugin.name()).collect()
    }

    /// Call every applicable plugin and collect snapshot records.
    ///
    /// Plugins that are not applicable for `cwd` are skipped silently.
    /// Plugin failures are logged as warnings and do not abort the loop.
    ///
    /// The returned [`SnapshotCoverage`] carries the applicable-plugin count
    /// from this same pass, so a caller can tell a complete attempt from a
    /// partial one without asking every plugin for applicability a second
    /// time and risking a different answer.
    pub async fn snapshot_all(&self, cwd: &Path, cmd: &str) -> SnapshotCoverage {
        let mut coverage = SnapshotCoverage::default();
        for plugin in &self.plugins {
            if !plugin.is_applicable(cwd).await {
                continue;
            }
            coverage.applicable += 1;
            match plugin.snapshot(cwd, cmd).await {
                Ok(snapshot_id) => coverage.records.push(SnapshotRecord {
                    plugin: plugin.name(),
                    snapshot_id,
                }),
                Err(e) => {
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

    /// Return the subset of configured providers that are applicable to `cwd`.
    ///
    /// This is a later-stage runtime-use check than either
    /// [`crate::available_provider_names`] (providers known to the binary/runtime) or
    /// [`SnapshotRegistry::configured_provider_names`] (providers materialized
    /// by the current runtime config). No snapshots are created.
    pub async fn applicable_plugins(&self, cwd: &Path) -> Vec<&'static str> {
        let mut names = Vec::new();
        for plugin in &self.plugins {
            if plugin.is_applicable(cwd).await {
                names.push(plugin.name());
            }
        }
        names
    }

    /// Roll back one snapshot using the named plugin.
    pub async fn rollback(
        &self,
        plugin_name: &str,
        snapshot_id: &str,
    ) -> std::result::Result<(), SnapshotError> {
        let plugin = self
            .plugins
            .iter()
            .find(|plugin| plugin.name() == plugin_name)
            .ok_or_else(|| {
                SnapshotError::Snapshot(format!(
                    "snapshot plugin {plugin_name:?} is not available for rollback"
                ))
            })?;

        plugin.rollback(snapshot_id).await
    }

    /// Delete one snapshot using the named plugin.
    pub async fn delete(
        &self,
        plugin_name: &str,
        snapshot_id: &str,
    ) -> std::result::Result<(), SnapshotError> {
        let plugin = self
            .plugins
            .iter()
            .find(|plugin| plugin.name() == plugin_name)
            .ok_or_else(|| {
                SnapshotError::Snapshot(format!(
                    "snapshot plugin {plugin_name:?} is not available for delete"
                ))
            })?;

        plugin.delete(snapshot_id).await
    }

    /// Resolve the snapshot records that are still on record and have not been
    /// pruned.
    ///
    /// Reads the default audit log (`~/.aegis/audit.jsonl`), collects the latest
    /// recorded timestamp for each `(plugin, snapshot_id)` pair, and removes
    /// any id that has a later `Decision::Pruned` entry. If the audit log is
    /// missing, the result is empty.
    pub async fn resolve_prunable_records(
        &self,
    ) -> std::result::Result<Vec<PrunableRecord>, SnapshotError> {
        resolve_prunable_records_from_default_audit_log()
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

    /// A plugin that is always applicable and always fails to snapshot, so
    /// `snapshot_all` reaches its warn arm deterministically.
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
    }

    /// The fields captured off the single `tracing` event the test expects.
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

    /// A minimal `tracing::Subscriber` that records the fields of the one
    /// event it expects to see, without pulling in `tracing-subscriber` as a
    /// dependency just for this test.
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

    /// A plugin that is always applicable and always succeeds.
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
    }

    /// A plugin that never applies, so it must not enter the applicable count.
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
    }

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
        // The command string itself must never appear in the tracing fields
        // (CONVENTION.md §2 — no raw command string in the Diagnostic stream).
        assert!(!error.contains("rm -rf /"));
        assert_ne!(captured.message.as_deref(), Some("rm -rf /"));
    }
}
