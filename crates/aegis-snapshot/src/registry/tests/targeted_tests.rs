use super::*;

/// A plugin that snapshots fine and reports a fixed target.
struct TargetedPlugin;

#[async_trait]
impl SnapshotPlugin for TargetedPlugin {
    fn name(&self) -> &'static str {
        "mock-targeted"
    }

    async fn is_applicable(&self, _cwd: &Path) -> bool {
        true
    }

    async fn snapshot(
        &self,
        _cwd: &Path,
        _cmd: &str,
    ) -> std::result::Result<String, SnapshotError> {
        Ok("id".to_string())
    }

    async fn snapshot_target(&self, _cwd: &Path) -> Option<String> {
        Some("/the/target".to_string())
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

    async fn liveness(&self, _snapshot_id: &str) -> std::result::Result<Liveness, SnapshotError> {
        Ok(Liveness::Present)
    }
}

#[test]
fn snapshot_all_records_the_target_a_plugin_reports() {
    let registry = SnapshotRegistry::new_with_plugins(vec![Box::new(TargetedPlugin)]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    let coverage = runtime.block_on(registry.snapshot_all(Path::new("/x"), "c"));

    assert_eq!(
        coverage.records[0].git_target.as_deref(),
        Some("/the/target")
    );
}
