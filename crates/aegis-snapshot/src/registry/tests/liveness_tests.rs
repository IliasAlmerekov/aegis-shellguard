use super::*;
/// A plugin whose liveness check never finishes.
struct HangingLivenessPlugin;

#[async_trait]
impl SnapshotPlugin for HangingLivenessPlugin {
    fn name(&self) -> &'static str {
        "mock-hanging"
    }

    async fn is_applicable(&self, _cwd: &Path) -> bool {
        true
    }

    async fn snapshot(
        &self,
        _cwd: &Path,
        _cmd: &str,
    ) -> std::result::Result<String, SnapshotError> {
        Ok(String::new())
    }

    async fn rollback(&self, _snapshot_id: &str) -> std::result::Result<(), SnapshotError> {
        Ok(())
    }

    async fn delete(&self, _snapshot_id: &str) -> std::result::Result<(), SnapshotError> {
        Ok(())
    }

    async fn liveness(&self, _snapshot_id: &str) -> std::result::Result<Liveness, SnapshotError> {
        std::future::pending().await
    }
}

#[tokio::test]
async fn liveness_within_reports_inconclusive_when_the_check_overruns() {
    let outcome = liveness_within(
        std::time::Duration::from_millis(20),
        &HangingLivenessPlugin,
        "id",
    )
    .await
    .unwrap();

    assert!(
        matches!(outcome, Liveness::Inconclusive { .. }),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn liveness_dispatches_to_the_named_plugin() {
    let registry = SnapshotRegistry::new_with_plugins(vec![
        Box::new(AlwaysFailingPlugin),
        Box::new(AlwaysSucceedingPlugin),
    ]);

    let outcome = registry.liveness("mock-succeeding", "id").await.unwrap();

    assert_eq!(outcome, Liveness::Present);
}

#[tokio::test]
async fn liveness_for_an_unknown_plugin_is_an_error() {
    let registry = SnapshotRegistry::new_with_plugins(vec![]);

    let error = registry.liveness("nope", "id").await.unwrap_err();

    assert!(
        error.to_string().contains("not available for liveness"),
        "{error}"
    );
}
