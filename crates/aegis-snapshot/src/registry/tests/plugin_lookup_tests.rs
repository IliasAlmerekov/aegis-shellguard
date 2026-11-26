use super::*;

struct RecordingPlugin {
    name: &'static str,
    calls: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl SnapshotPlugin for RecordingPlugin {
    fn name(&self) -> &'static str {
        self.name
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

    async fn rollback(&self, snapshot_id: &str) -> std::result::Result<(), SnapshotError> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("{}:rollback:{snapshot_id}", self.name));
        Ok(())
    }

    async fn delete(&self, snapshot_id: &str) -> std::result::Result<(), SnapshotError> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("{}:delete:{snapshot_id}", self.name));
        Ok(())
    }

    async fn readiness(&self, _snapshot_id: &str) -> ArtifactReadiness {
        ArtifactReadiness::PRESENT_UNCHECKED
    }

    async fn liveness(&self, _snapshot_id: &str) -> std::result::Result<Liveness, SnapshotError> {
        Ok(Liveness::Present)
    }
}

fn two_recording_plugins() -> (SnapshotRegistry, Arc<Mutex<Vec<String>>>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let plugin = |name| -> Box<dyn SnapshotPlugin> {
        Box::new(RecordingPlugin {
            name,
            calls: Arc::clone(&calls),
        })
    };
    let registry = SnapshotRegistry::new_with_plugins(vec![plugin("first"), plugin("second")]);
    (registry, calls)
}

#[tokio::test]
async fn rollback_reaches_only_the_named_plugin() {
    let (registry, calls) = two_recording_plugins();
    registry.rollback("second", "id-1").await.unwrap();
    assert_eq!(*calls.lock().unwrap(), vec!["second:rollback:id-1"]);
}

#[tokio::test]
async fn delete_reaches_only_the_named_plugin() {
    let (registry, calls) = two_recording_plugins();
    registry.delete("second", "id-1").await.unwrap();
    assert_eq!(*calls.lock().unwrap(), vec!["second:delete:id-1"]);
}

#[tokio::test]
async fn rollback_reports_unavailable_plugin_as_typed_error() {
    let (registry, calls) = two_recording_plugins();
    let error = registry.rollback("missing", "id").await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "snapshot error: snapshot plugin \"missing\" is not available for rollback"
    );
    assert!(matches!(
        error,
        SnapshotError::PluginUnavailable {
            operation: "rollback",
            ..
        }
    ));
    assert!(calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn delete_reports_unavailable_plugin_as_typed_error() {
    let (registry, calls) = two_recording_plugins();
    let error = registry.delete("missing", "id").await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "snapshot error: snapshot plugin \"missing\" is not available for delete"
    );
    assert!(matches!(
        error,
        SnapshotError::PluginUnavailable {
            operation: "delete",
            ..
        }
    ));
    assert!(calls.lock().unwrap().is_empty());
}
