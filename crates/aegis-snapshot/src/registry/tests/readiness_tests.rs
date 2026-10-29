use super::*;
use aegis_types::{SnapshotAttempt, SnapshotAttemptReadiness, StructuralCheck, ValidationLevel};
use std::time::{Duration, Instant};

/// A plugin whose snapshot succeeds or fails on demand and whose readiness
/// answers `verdict` after `delay` (`None`: never answers).
struct ScriptedPlugin {
    name: &'static str,
    snapshot_ok: bool,
    verdict: ArtifactReadiness,
    delay: Option<Duration>,
}

#[async_trait]
impl SnapshotPlugin for ScriptedPlugin {
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
        if self.snapshot_ok {
            Ok(format!("{}-id", self.name))
        } else {
            Err(SnapshotError::Snapshot("scripted failure".to_string()))
        }
    }

    async fn rollback(&self, _snapshot_id: &str) -> std::result::Result<(), SnapshotError> {
        Ok(())
    }

    async fn delete(&self, _snapshot_id: &str) -> std::result::Result<(), SnapshotError> {
        Ok(())
    }

    async fn liveness(&self, _snapshot_id: &str) -> std::result::Result<Liveness, SnapshotError> {
        Ok(Liveness::Present)
    }

    async fn readiness(&self, _snapshot_id: &str) -> ArtifactReadiness {
        match self.delay {
            Some(delay) => tokio::time::sleep(delay).await,
            None => std::future::pending().await,
        }
        self.verdict
    }
}

const READY: ArtifactReadiness = ArtifactReadiness::NOT_SUPPORTED;

fn scripted(name: &'static str, verdict: ArtifactReadiness) -> Box<ScriptedPlugin> {
    Box::new(ScriptedPlugin {
        name,
        snapshot_ok: true,
        verdict,
        delay: Some(Duration::ZERO),
    })
}

fn triple(
    attempt: &SnapshotAttempt,
) -> (SnapshotAttemptReadiness, ValidationLevel, StructuralCheck) {
    (
        attempt.readiness,
        attempt.validation_level,
        attempt.structural_check,
    )
}

async fn checked_pass(registry: &SnapshotRegistry) -> SnapshotCoverage {
    let mut coverage = registry.snapshot_all(Path::new("/x"), "c").await;
    registry.check_readiness(&mut coverage).await;
    coverage
}

#[tokio::test]
async fn each_record_gets_the_attempt_its_plugin_reports() {
    let registry = SnapshotRegistry::new_with_plugins(vec![
        scripted("ready", READY),
        scripted("corrupt", ArtifactReadiness::EMPTY),
    ]);

    let coverage = checked_pass(&registry).await;

    let attempts = coverage.snapshot_attempts();
    assert_eq!(
        attempts
            .iter()
            .map(|a| (a.plugin.as_str(), a.readiness))
            .collect::<Vec<_>>(),
        vec![
            ("ready", SnapshotAttemptReadiness::Ready),
            ("corrupt", SnapshotAttemptReadiness::Invalid)
        ]
    );
}

#[tokio::test]
async fn a_plugin_that_produced_no_record_is_an_unavailable_not_run_attempt() {
    let registry = SnapshotRegistry::new_with_plugins(vec![
        scripted("ready", READY),
        Box::new(ScriptedPlugin {
            name: "failed",
            snapshot_ok: false,
            verdict: READY,
            delay: Some(Duration::ZERO),
        }),
    ]);

    let coverage = checked_pass(&registry).await;

    let attempts = coverage.snapshot_attempts();
    let failed = attempts.iter().find(|a| a.plugin == "failed").unwrap();
    assert_eq!(
        triple(failed),
        (
            SnapshotAttemptReadiness::Unavailable,
            ValidationLevel::PresenceOnly,
            StructuralCheck::NotRun
        )
    );
}

#[tokio::test]
async fn a_presence_check_that_never_finishes_is_unavailable_within_the_attempt_budget() {
    let registry = SnapshotRegistry::new_with_plugins(vec![Box::new(ScriptedPlugin {
        name: "hanging",
        snapshot_ok: true,
        verdict: READY,
        delay: None,
    })]);

    let started = Instant::now();
    let coverage = checked_pass(&registry).await;
    let elapsed = started.elapsed();

    assert_eq!(
        triple(&coverage.snapshot_attempts()[0]),
        (
            SnapshotAttemptReadiness::Unavailable,
            ValidationLevel::PresenceOnly,
            StructuralCheck::NotRun
        )
    );
    assert!(elapsed < Duration::from_millis(400), "{elapsed:?}");
}

#[tokio::test]
async fn a_structural_timeout_reported_by_the_plugin_stays_ready() {
    let timed_out = ArtifactReadiness {
        structural_check: StructuralCheck::TimedOut,
        ..READY
    };
    let registry = SnapshotRegistry::new_with_plugins(vec![scripted("slow-tool", timed_out)]);

    let coverage = checked_pass(&registry).await;

    assert_eq!(
        triple(&coverage.snapshot_attempts()[0]),
        (
            SnapshotAttemptReadiness::Ready,
            ValidationLevel::PresenceOnly,
            StructuralCheck::TimedOut
        )
    );
}

#[tokio::test]
async fn all_checks_of_one_command_share_a_500ms_budget_and_fail_closed() {
    let registry = SnapshotRegistry::new_with_plugins(vec![Box::new(ScriptedPlugin {
        name: "hanging",
        snapshot_ok: true,
        verdict: READY,
        delay: None,
    })]);
    let record = SnapshotRecord {
        plugin: "hanging",
        snapshot_id: "id".to_string(),
        git_target: None,
    };
    let mut coverage = SnapshotCoverage {
        records: vec![record; 20],
        applicable: 20,
        ..SnapshotCoverage::default()
    };

    let started = Instant::now();
    registry.check_readiness(&mut coverage).await;
    let elapsed = started.elapsed();

    assert!(elapsed < Duration::from_millis(800), "{elapsed:?}");
    assert!(
        coverage.attempts.len() == 20
            && coverage
                .attempts
                .iter()
                .all(|a| a.readiness == SnapshotAttemptReadiness::Unavailable)
    );
}

#[tokio::test]
async fn merged_passes_carry_every_attempt() {
    let registry = SnapshotRegistry::new_with_plugins(vec![scripted("a", READY)]);
    let mut first = checked_pass(&registry).await;
    let second = checked_pass(&registry).await;

    first.merge(second);

    assert_eq!(first.snapshot_attempts().len(), 2);
}
