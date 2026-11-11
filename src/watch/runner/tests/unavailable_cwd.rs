use super::*;

/// A snapshot plugin that fails the test if probed or invoked.
struct ForbiddenPlugin;

#[async_trait::async_trait]
impl aegis_snapshot::SnapshotPlugin for ForbiddenPlugin {
    fn name(&self) -> &'static str {
        "mock-succeeding"
    }

    async fn is_applicable(&self, _cwd: &std::path::Path) -> bool {
        panic!("snapshot applicability must not run")
    }

    async fn snapshot(
        &self,
        _cwd: &std::path::Path,
        _cmd: &str,
    ) -> Result<String, aegis_snapshot::SnapshotError> {
        panic!("snapshot must not run")
    }

    async fn rollback(&self, _snapshot_id: &str) -> Result<(), aegis_snapshot::SnapshotError> {
        Ok(())
    }

    async fn delete(&self, _snapshot_id: &str) -> Result<(), aegis_snapshot::SnapshotError> {
        Ok(())
    }

    async fn readiness(&self, _snapshot_id: &str) -> aegis_snapshot::ArtifactReadiness {
        aegis_snapshot::ArtifactReadiness::PRESENT_UNCHECKED
    }

    async fn liveness(
        &self,
        _snapshot_id: &str,
    ) -> Result<aegis_snapshot::Liveness, aegis_snapshot::SnapshotError> {
        Ok(aegis_snapshot::Liveness::Present)
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn unavailable_cwd_denies_watch_and_audits_recovery_degradation() {
    let audit_dir = TempDir::new().unwrap();
    let audit_path = audit_dir.path().join("audit.jsonl");
    let context = RuntimeContext::new_with_audit_path(
        AegisConfig::default(),
        tokio::runtime::Handle::current(),
        audit_path.clone(),
    )
    .unwrap();
    context.set_snapshot_registry_for_tests(SnapshotRegistry::new_with_plugins(vec![Box::new(
        ForbiddenPlugin,
    )]));
    let prepared = PreparedPlanner::Ready(Box::new(context));
    let PlanningOutcome::Planned(shell_plan) = prepared
        .plan_async(PlanningRequest {
            command: "rm -rf target",
            cwd_state: CwdState::Unavailable,
            transport: ExecutionTransport::Shell,
            ci_detected: false,
        })
        .await
    else {
        panic!("expected shell plan")
    };
    assert!(
        shell_plan
            .decision_context()
            .applicable_snapshot_plugins()
            .is_empty()
    );
    let PlanningOutcome::Planned(plan) = prepared
        .plan_async(PlanningRequest {
            command: "rm -rf target",
            cwd_state: CwdState::Unavailable,
            transport: ExecutionTransport::Watch,
            ci_detected: false,
        })
        .await
    else {
        panic!("expected plan")
    };
    let prompted = Cell::new(None);
    run_watch_plan_with_prompts(
        InputFrame {
            cmd: "rm -rf target".into(),
            cwd: None,
            interactive: None,
            source: None,
            id: None,
        },
        &prepared,
        plan,
        false,
        |_, _| PromptDecision::Approve,
        |degradation| {
            prompted.set(Some(degradation));
            RecoveryPromptDecision::Deny
        },
    )
    .await;
    assert_eq!(
        prompted.get(),
        Some(aegis_types::RecoveryDegradation::UnavailableCwd)
    );
    let entry = read_audit_entry(&audit_path);
    assert_eq!(entry["decision"], "Denied");
    assert_eq!(entry["recovery_degradation"], "unavailable_cwd");
    assert_eq!(entry["cwd"], Value::Null);
}
