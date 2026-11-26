use aegis::error::AegisError;
use aegis_audit::{AuditEntry, AuditLogger, AuditSnapshot, SnapshotLedger};
use aegis_config::{AegisConfig, PruneConfig};
use aegis_snapshot::{Clock, PrunableRecord, RetentionPolicy, SnapshotRegistry, SystemClock};
use aegis_types::Decision;
use aegis_types::RiskLevel;

pub(crate) async fn execute(args: crate::PruneArgs) -> Result<Vec<PrunableRecord>, AegisError> {
    let config = AegisConfig::load().map_err(AegisError::from)?;
    let registry = SnapshotRegistry::for_rollback().map_err(AegisError::from)?;
    let logger = AuditLogger::from_audit_config(&config.audit);
    let ledger = SnapshotLedger::load(&logger)?;
    let records = registry.resolve_prunable_records(&ledger.live_snapshots());
    let policy = RetentionPolicy::from_config(&config.prune);
    let now = SystemClock.now();
    let candidates = policy.apply(&records, now);

    if !args.yes || args.dry_run {
        preview_candidates(&candidates, &config.prune);
        return Ok(Vec::new());
    }

    let mut pruned = Vec::new();
    let mut failures = Vec::new();
    for record in candidates {
        match registry.delete(&record.plugin, &record.snapshot_id).await {
            Ok(()) => {
                let entry = AuditEntry::new(
                    format!("aegis prune {}", record.snapshot_id),
                    RiskLevel::Safe,
                    Vec::new(),
                    Decision::Pruned,
                    vec![AuditSnapshot {
                        plugin: record.plugin.clone(),
                        snapshot_id: record.snapshot_id.clone(),
                        git_target: None,
                    }],
                    None,
                    None,
                );
                logger.append(entry).map_err(AegisError::from)?;
                pruned.push(record);
            }
            Err(error) => {
                tracing::warn!(
                    plugin = %record.plugin,
                    id = %record.snapshot_id,
                    error = %error,
                    "prune delete failed"
                );
                failures.push(format!(
                    "{} snapshot {}: {error}",
                    record.plugin, record.snapshot_id
                ));
            }
        }
    }

    if !failures.is_empty() {
        return Err(AegisError::PrunePartialFailure {
            pruned: pruned.len(),
            failed_count: failures.len(),
            failed: failures.join("\n"),
        });
    }

    out!("Pruned {} snapshot(s).", pruned.len());
    Ok(pruned)
}

fn preview_candidates(candidates: &[PrunableRecord], prune_config: &PruneConfig) {
    if candidates.is_empty() {
        out!("No snapshots are eligible for pruning.");
        return;
    }

    if !prune_config.enabled {
        out!("Note: prune is currently disabled in config; this is a dry-run preview.");
    }

    out!("Would prune {} snapshot(s):", candidates.len());
    for record in candidates {
        out!("  provider: {}  id: {}", record.plugin, record.snapshot_id);
    }
}
