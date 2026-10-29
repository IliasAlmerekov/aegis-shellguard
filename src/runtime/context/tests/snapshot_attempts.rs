//! A Shell entry and a Watch entry for one Snapshot pass carry the same
//! `snapshot_attempts` (ADR-031 §10).

use super::*;
use aegis_snapshot::{ArtifactReadiness, Liveness, SnapshotError, SnapshotPlugin};
use aegis_types::SnapshotAttemptReadiness;

/// A plugin that applies, snapshots on demand, and judges its artifact `Invalid`.
struct Fake {
    name: &'static str,
    snapshot_ok: bool,
}

#[async_trait::async_trait]
impl SnapshotPlugin for Fake {
    fn name(&self) -> &'static str {
        self.name
    }
    async fn is_applicable(&self, _cwd: &Path) -> bool {
        true
    }
    async fn snapshot(&self, _cwd: &Path, _cmd: &str) -> Result<String, SnapshotError> {
        if self.snapshot_ok {
            Ok("id".to_string())
        } else {
            Err(SnapshotError::Snapshot("scripted failure".to_string()))
        }
    }
    async fn rollback(&self, _id: &str) -> Result<(), SnapshotError> {
        Ok(())
    }
    async fn delete(&self, _id: &str) -> Result<(), SnapshotError> {
        Ok(())
    }
    async fn liveness(&self, _id: &str) -> Result<Liveness, SnapshotError> {
        Ok(Liveness::Present)
    }
    async fn readiness(&self, _id: &str) -> ArtifactReadiness {
        ArtifactReadiness::EMPTY
    }
}

fn explanation(context: &RuntimeContext, assessment: &Assessment) -> CommandExplanation {
    CommandExplanation {
        scan: ScanExplanation {
            highest_risk: assessment.risk,
            decision_source: assessment.decision_source(),
            basis: assessment.basis(),
            matched_patterns: Vec::new(),
        },
        policy: PolicyExplanation {
            action: PolicyAction::Prompt,
            rationale: PolicyRationale::RequiresConfirmation,
            requires_confirmation: true,
            snapshots_required: true,
            allowlist_effective: false,
            block_reason: None,
        },
        context: ExecutionContextExplanation {
            mode: context.config().mode,
            transport: ExecutionTransport::Shell,
            ci_detected: false,
            allowlist_match: None,
            applicable_snapshot_plugins: Vec::new(),
            git_targets: Vec::new(),
        },
        outcome: None,
    }
}

#[test]
fn shell_and_watch_entries_for_one_pass_carry_the_same_attempts() {
    let dir = TempDir::new().unwrap();
    let audit_path = dir.path().join("audit.jsonl");
    let context = RuntimeContext::new_with_audit_path(
        AegisConfig::default(),
        test_handle(),
        audit_path.clone(),
    )
    .unwrap();
    context.set_snapshot_registry_for_tests(aegis_snapshot::SnapshotRegistry::new_with_plugins(
        vec![
            Box::new(Fake {
                name: "recorded",
                snapshot_ok: true,
            }),
            Box::new(Fake {
                name: "failed",
                snapshot_ok: false,
            }),
        ],
    ));
    let assessment = context.assess("sh ./cleanup.sh");
    let explanation = explanation(&context, &assessment);

    let coverage = context.create_snapshots(Path::new("."), "sh ./cleanup.sh", false);
    let attempts = coverage.snapshot_attempts();
    context
        .append_audit_entry(
            &assessment,
            Decision::Approved,
            &coverage.records,
            &explanation,
            AuditWriteOptions {
                allowlist_match: None,
                allowlist_effective: false,
                ci_detected: false,
                sandbox_status: SandboxStatus::NotConfigured,
                snapshot_attempts: &attempts,
            },
        )
        .unwrap();
    context
        .append_watch_audit_entry(
            &assessment,
            Decision::Approved,
            &coverage.records,
            &explanation,
            WatchAuditContext {
                allowlist_match: None,
                allowlist_effective: false,
                ci_detected: false,
                sandbox_status: SandboxStatus::NotConfigured,
                snapshot_attempts: &attempts,
                source: None,
                cwd: None,
                id: None,
            },
        )
        .unwrap();

    let entries = AuditLogger::new(audit_path).read_all().unwrap();
    let recorded: Vec<_> = entries
        .iter()
        .map(|entry| entry.as_base().snapshot_attempts.clone())
        .collect();
    assert_eq!(recorded.len(), 2);
    assert_eq!(recorded[0], recorded[1]);
    assert_eq!(
        recorded[0]
            .as_ref()
            .unwrap()
            .iter()
            .map(|a| (a.plugin.as_str(), a.readiness))
            .collect::<Vec<_>>(),
        vec![
            ("recorded", SnapshotAttemptReadiness::Invalid),
            ("failed", SnapshotAttemptReadiness::Unavailable),
        ]
    );
}

#[test]
fn an_entry_written_without_a_snapshot_pass_has_no_snapshot_attempts() {
    let dir = TempDir::new().unwrap();
    let audit_path = dir.path().join("audit.jsonl");
    let context = RuntimeContext::new_with_audit_path(
        AegisConfig::default(),
        test_handle(),
        audit_path.clone(),
    )
    .unwrap();
    let assessment = context.assess("rm -rf target");

    context
        .append_audit_entry(
            &assessment,
            Decision::Denied,
            &[],
            &explanation(&context, &assessment),
            AuditWriteOptions {
                allowlist_match: None,
                allowlist_effective: false,
                ci_detected: false,
                sandbox_status: SandboxStatus::NotConfigured,
                snapshot_attempts: &[],
            },
        )
        .unwrap();

    let entries = AuditLogger::new(audit_path).read_all().unwrap();
    assert_eq!(entries[0].as_base().snapshot_attempts, None);
}
