use aegis::error::AegisError;
use aegis_audit::{AuditEntry, AuditLogger, AuditSnapshot, SnapshotLedger, SnapshotLookupError};
use aegis_config::AegisConfig;
use aegis_snapshot::{Liveness, SnapshotRegistry, SnapshotRegistryConfig};
use aegis_types::Decision;
use std::io::{BufRead, IsTerminal};

use crate::{EXIT_ABSENT, EXIT_DENIED};
use aegis_types::RiskLevel;

type Result<T> = std::result::Result<T, AegisError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RollbackTarget {
    pub(crate) plugin: String,
    pub(crate) snapshot_id: String,
}

/// What `aegis rollback` did: restored the Snapshot, or refused before writing anything.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RollbackOutcome {
    Restored(RollbackTarget),
    Refused { code: i32, message: String },
}

/// The liveness preflight's verdict (ADR-052 §6, §7).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Preflight {
    Restore,
    Refused { code: i32, message: String },
}

pub(crate) async fn execute(snapshot_id: String, yes: bool) -> Result<RollbackOutcome> {
    let config = AegisConfig::load()?;
    let audit_logger = AuditLogger::from_audit_config(&config.audit);
    let target = find_snapshot_target(&audit_logger, &snapshot_id)?;

    let registry = SnapshotRegistry::from_runtime_config(
        &SnapshotRegistryConfig::for_rollback_from_config(&config)?,
    );
    let liveness = registry
        .liveness(&target.plugin, &target.snapshot_id)
        .await?;
    let is_tty = std::io::stdin().is_terminal();
    if let Preflight::Refused { code, message } =
        decide(liveness, yes, is_tty, &mut std::io::stdin().lock())
    {
        return Ok(RollbackOutcome::Refused { code, message });
    }

    registry
        .rollback(&target.plugin, &target.snapshot_id)
        .await?;

    append_rollback_audit_entry(&audit_logger, &target)?;

    Ok(RollbackOutcome::Restored(target))
}

/// Decide from the liveness outcome whether Rollback may write. Only a
/// question needs a terminal: `Inconclusive` asks, everything else does not.
pub(crate) fn decide(
    liveness: Liveness,
    yes: bool,
    is_tty: bool,
    input: &mut impl BufRead,
) -> Preflight {
    match liveness {
        Liveness::Present | Liveness::NothingCaptured => Preflight::Restore,
        Liveness::Absent { reason } => Preflight::Refused {
            code: EXIT_ABSENT,
            message: format!(
                "the Snapshot artifact is confirmed gone and the restore would fail: {reason}"
            ),
        },
        Liveness::Inconclusive { reason } => {
            if yes {
                return Preflight::Restore;
            }
            if !is_tty {
                return Preflight::Refused {
                    code: EXIT_DENIED,
                    message: format!(
                        "the Snapshot liveness check could not complete ({reason}) and there is no terminal to ask; pass --yes to attempt the restore anyway"
                    ),
                };
            }
            eprint!(
                "The Snapshot liveness check could not complete ({reason}).\nAttempt the restore anyway? [y/N] "
            );
            let mut line = String::new();
            // A read error counts as "no", like EOF.
            let _ = input.read_line(&mut line);
            if matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
                Preflight::Restore
            } else {
                Preflight::Refused {
                    code: EXIT_DENIED,
                    message: "rollback cancelled".to_string(),
                }
            }
        }
    }
}

pub(crate) fn find_snapshot_target(
    logger: &AuditLogger,
    snapshot_id: &str,
) -> Result<RollbackTarget> {
    let ledger = SnapshotLedger::load(logger)?;
    match ledger.resolve(snapshot_id) {
        Ok(record) => Ok(RollbackTarget {
            plugin: record.plugin.clone(),
            snapshot_id: record.snapshot_id.clone(),
        }),
        Err(SnapshotLookupError::Pruned) => Err(AegisError::SnapshotPruned {
            snapshot_id: snapshot_id.to_string(),
        }),
        Err(SnapshotLookupError::NotFound) => Err(AegisError::SnapshotIdNotFound {
            snapshot_id: snapshot_id.to_string(),
        }),
    }
}

pub(crate) fn append_rollback_audit_entry(
    logger: &AuditLogger,
    target: &RollbackTarget,
) -> Result<()> {
    Ok(logger.append(AuditEntry::new(
        format!("aegis rollback {}", target.snapshot_id),
        RiskLevel::Safe,
        Vec::new(),
        Decision::Approved,
        vec![AuditSnapshot {
            plugin: target.plugin.clone(),
            snapshot_id: target.snapshot_id.clone(),
            git_target: None,
        }],
        None,
        None,
    ))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_entry(
        logger: &AuditLogger,
        command: &str,
        plugin: &str,
        snapshot_id: &str,
    ) -> Result<()> {
        Ok(logger.append(AuditEntry::new(
            command,
            RiskLevel::Danger,
            Vec::new(),
            Decision::Denied,
            vec![AuditSnapshot {
                plugin: plugin.to_string(),
                snapshot_id: snapshot_id.to_string(),
                git_target: None,
            }],
            None,
            None,
        ))?)
    }

    fn write_pruned_entry(
        logger: &AuditLogger,
        command: &str,
        plugin: &str,
        snapshot_id: &str,
    ) -> Result<()> {
        Ok(logger.append(AuditEntry::new(
            command,
            RiskLevel::Safe,
            Vec::new(),
            Decision::Pruned,
            vec![AuditSnapshot {
                plugin: plugin.to_string(),
                snapshot_id: snapshot_id.to_string(),
                git_target: None,
            }],
            None,
            None,
        ))?)
    }

    #[test]
    fn find_snapshot_target_rejects_pruned_snapshot() {
        let dir = TempDir::new().unwrap();
        let logger = AuditLogger::new(dir.path().join("audit.jsonl"));

        write_entry(&logger, "rm -rf src", "git", "snap-001").unwrap();
        write_pruned_entry(&logger, "aegis prune snap-001", "git", "snap-001").unwrap();

        let err = find_snapshot_target(&logger, "snap-001")
            .expect_err("a pruned snapshot must not be discoverable for rollback");
        let message = err.to_string();
        assert!(
            message.to_lowercase().contains("pruned"),
            "error must indicate the snapshot was pruned: {message}"
        );
    }

    #[test]
    fn find_snapshot_target_prefers_latest_matching_entry() {
        let dir = TempDir::new().unwrap();
        let logger = AuditLogger::new(dir.path().join("audit.jsonl"));

        write_entry(&logger, "first", "git", "snap-001").unwrap();
        write_entry(&logger, "second", "docker", "snap-001").unwrap();

        let target = find_snapshot_target(&logger, "snap-001").unwrap();
        assert_eq!(target.plugin, "docker");
    }

    #[test]
    fn find_snapshot_target_returns_recovery_hint_when_missing() {
        let dir = TempDir::new().unwrap();
        let logger = AuditLogger::new(dir.path().join("audit.jsonl"));

        let err = find_snapshot_target(&logger, "missing").expect_err("lookup must fail");
        let message = err.to_string();

        assert!(message.contains("missing"));
        assert!(message.contains("aegis audit"));
    }

    fn inconclusive() -> Liveness {
        Liveness::Inconclusive {
            reason: "docker daemon did not answer".to_string(),
        }
    }

    fn decide_with(liveness: Liveness, yes: bool, is_tty: bool, typed: &str) -> Preflight {
        decide(liveness, yes, is_tty, &mut typed.as_bytes())
    }

    fn refused_code(preflight: &Preflight) -> Option<i32> {
        match preflight {
            Preflight::Refused { code, .. } => Some(*code),
            Preflight::Restore => None,
        }
    }

    #[test]
    fn decide_restores_present_and_nothing_captured_without_asking() {
        for liveness in [Liveness::Present, Liveness::NothingCaptured] {
            assert_eq!(decide_with(liveness, false, false, ""), Preflight::Restore);
        }
    }

    #[test]
    fn decide_refuses_absent_with_exit_1_even_with_yes() {
        let absent = || Liveness::Absent {
            reason: "stash entry gone".to_string(),
        };
        for (yes, tty, typed) in [(false, false, ""), (true, false, ""), (true, true, "y\n")] {
            let preflight = decide_with(absent(), yes, tty, typed);
            assert_eq!(refused_code(&preflight), Some(1), "{preflight:?}");
        }
    }

    #[test]
    fn decide_proceeds_on_inconclusive_when_the_operator_types_y_or_yes() {
        for typed in ["y\n", "yes\n", "Y\n", " YES \n"] {
            assert_eq!(
                decide_with(inconclusive(), false, true, typed),
                Preflight::Restore,
                "{typed:?}"
            );
        }
    }

    #[test]
    fn decide_refuses_inconclusive_with_exit_2_on_other_input() {
        for typed in ["n\n", "\n", "", "maybe\n"] {
            let preflight = decide_with(inconclusive(), false, true, typed);
            assert_eq!(refused_code(&preflight), Some(2), "{typed:?}");
        }
    }

    #[test]
    fn decide_refuses_inconclusive_without_a_terminal_and_names_yes() {
        let preflight = decide_with(inconclusive(), false, false, "y\n");
        match preflight {
            Preflight::Refused { code, message } => {
                assert_eq!(code, 2);
                assert!(message.contains("--yes"), "{message}");
            }
            Preflight::Restore => panic!("must fail closed without a TTY"),
        }
    }

    #[test]
    fn decide_skips_the_question_for_inconclusive_with_yes() {
        for is_tty in [true, false] {
            assert_eq!(
                decide_with(inconclusive(), true, is_tty, ""),
                Preflight::Restore
            );
        }
    }
}
