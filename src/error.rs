use aegis_audit::error::AuditError;
use aegis_scanner::ScannerError;
use aegis_snapshot::SnapshotError;

use crate::config::error::ConfigError;

/// Typed error hierarchy for all Aegis operations.
///
/// Each library crate already carries a precise, typed error, for example a
/// Path containment violation, a corrupted Audit log, or a missing Docker
/// daemon. This type wraps those lower errors transparently instead of
/// mirroring their variants or flattening them into strings, so a Path
/// containment violation stays matchable by variant all the way to the CLI.
/// The binary never has to parse message text to recover information a
/// lower crate already had.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum AegisError {
    /// Snapshot creation, rollback, or deletion failed.
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),

    /// Audit log read, write, or integrity verification failed.
    #[error(transparent)]
    Audit(#[from] AuditError),

    /// Command scanning or custom pattern-set construction failed.
    #[error(transparent)]
    Scanner(#[from] ScannerError),

    /// Configuration loading, parsing, or validation failed.
    #[error(transparent)]
    Config(#[from] ConfigError),

    /// The requested snapshot id was pruned and is no longer recoverable.
    #[error("snapshot id {snapshot_id:?} has been pruned and is no longer recoverable.")]
    SnapshotPruned {
        /// Snapshot id that was pruned.
        snapshot_id: String,
    },

    /// The requested snapshot id has no matching entry in the audit log.
    #[error(
        "snapshot id {snapshot_id:?} was not found in the audit log.\n\
         Hint: run `aegis audit --format json` or `aegis audit --last 20` \
         to find a recorded snapshot id, then retry `aegis rollback <snapshot-id>`."
    )]
    SnapshotIdNotFound {
        /// Snapshot id that could not be resolved.
        snapshot_id: String,
    },

    /// One or more prune candidates could not be deleted.
    #[error(
        "prune completed with failures: {failed_count} candidate(s) could not be deleted. \
         {pruned} snapshot(s) were pruned successfully.\n{failed}"
    )]
    PrunePartialFailure {
        /// Number of snapshots successfully pruned.
        pruned: usize,
        /// Number of snapshots that could not be deleted.
        failed_count: usize,
        /// Human-readable descriptions of each failed deletion.
        failed: String,
    },

    /// A fault in the binary's own orchestration logic, not something a user
    /// fixes by editing `aegis.toml`.
    #[error("internal error: {detail}")]
    Internal {
        /// Description of the internal fault.
        detail: String,
    },

    /// Wrapped I/O error from the standard library.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl AegisError {
    /// Whether this error stems from invalid or unreadable user configuration
    /// rather than a fault in Aegis itself.
    ///
    /// `true` for `Config` and for `Scanner(ScannerError::InvalidPattern)`,
    /// a custom pattern the user wrote in their config. `false` for
    /// everything else, including `Scanner(ScannerError::Build)`,
    /// `Internal`, a corrupted Audit log, and `Snapshot(SnapshotError::Config)`
    /// (an unset `HOME`). None of those are fixed by editing `aegis.toml`;
    /// an unset environment variable, for instance, is not a config problem.
    pub fn is_config_fault(&self) -> bool {
        matches!(
            self,
            Self::Config(_) | Self::Scanner(ScannerError::InvalidPattern { .. })
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_escapes_snapshot_store_message_has_no_snapshot_error_prefix() {
        let err = AegisError::Snapshot(SnapshotError::PathEscapesSnapshotStore {
            plugin: "git",
            store: "/home/user/.aegis/snapshots".to_string(),
            candidate: "/etc/passwd".to_string(),
        });
        assert_eq!(
            err.to_string(),
            "git snapshot artifact '/etc/passwd' escapes snapshot store '/home/user/.aegis/snapshots'"
        );
    }

    #[test]
    fn insecure_snapshot_permissions_message_has_no_snapshot_error_prefix() {
        let err = AegisError::Snapshot(SnapshotError::InsecureSnapshotPermissions {
            plugin: "docker".to_string(),
            path: "/home/user/.aegis/snapshots/docker-1".to_string(),
            detail: "mode 0644, expected 0600".to_string(),
        });
        assert_eq!(
            err.to_string(),
            "docker snapshot path '/home/user/.aegis/snapshots/docker-1' does not meet owner-only permissions: mode 0644, expected 0600"
        );
    }

    #[test]
    fn delete_failed_message_uses_delete_failed_for_wording() {
        let err = AegisError::Snapshot(SnapshotError::DeleteFailed {
            plugin: "git".to_string(),
            snapshot_id: "snap-001".to_string(),
            source: "git stash drop failed".to_string(),
        });
        assert_eq!(
            err.to_string(),
            "delete failed for git snapshot snap-001: git stash drop failed"
        );
    }

    #[test]
    fn corrupted_audit_log_message_is_audit_error_not_config_error() {
        let source = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err = AegisError::Audit(AuditError::Parse {
            path: "/home/user/.aegis/audit.jsonl".to_string(),
            line: Some(3),
            source,
        });
        assert_eq!(
            err.to_string(),
            "audit error: failed to parse audit log line 3 in /home/user/.aegis/audit.jsonl: expected ident at line 1 column 2"
        );
    }

    #[test]
    fn path_containment_violation_is_matchable_by_variant() {
        let err = AegisError::Snapshot(SnapshotError::PathEscapesSnapshotStore {
            plugin: "git",
            store: "/store".to_string(),
            candidate: "/etc/passwd".to_string(),
        });
        assert!(matches!(
            err,
            AegisError::Snapshot(SnapshotError::PathEscapesSnapshotStore { .. })
        ));
    }

    #[test]
    fn owner_only_permission_violation_is_matchable_by_variant() {
        let err = AegisError::Snapshot(SnapshotError::InsecureSnapshotPermissions {
            plugin: "docker".to_string(),
            path: "/path".to_string(),
            detail: "detail".to_string(),
        });
        assert!(matches!(
            err,
            AegisError::Snapshot(SnapshotError::InsecureSnapshotPermissions { .. })
        ));
    }

    #[test]
    fn corrupted_audit_log_is_not_a_config_fault() {
        let source = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err = AegisError::Audit(AuditError::Parse {
            path: "/home/user/.aegis/audit.jsonl".to_string(),
            line: None,
            source,
        });
        assert!(!err.is_config_fault());
    }

    #[test]
    fn config_error_is_a_config_fault() {
        let err = AegisError::Config(ConfigError::Config("bad field".to_string()));
        assert!(err.is_config_fault());
    }

    #[test]
    fn invalid_custom_pattern_is_a_config_fault_and_keeps_the_pattern_id() {
        let err = AegisError::Scanner(ScannerError::InvalidPattern {
            id: "CUSTOM-001".to_string(),
            reason: "missing description".to_string(),
        });
        assert!(err.is_config_fault());
        match err {
            AegisError::Scanner(ScannerError::InvalidPattern { id, .. }) => {
                assert_eq!(id, "CUSTOM-001");
            }
            _ => panic!("expected AegisError::Scanner(ScannerError::InvalidPattern)"),
        }
    }

    #[test]
    fn scanner_build_failure_is_not_a_config_fault() {
        let err = AegisError::Scanner(ScannerError::Build("bad embedded TOML".to_string()));
        assert!(!err.is_config_fault());
    }

    #[test]
    fn internal_error_is_not_a_config_fault() {
        let err = AegisError::Internal {
            detail: "custom scanner cache lock poisoned".to_string(),
        };
        assert!(!err.is_config_fault());
    }

    #[test]
    fn snapshot_config_missing_home_message_has_the_snapshot_config_error_prefix() {
        let err = AegisError::Snapshot(SnapshotError::Config(
            "HOME is not set; cannot determine snapshot storage directory".to_string(),
        ));
        assert_eq!(
            err.to_string(),
            "snapshot config error: HOME is not set; cannot determine snapshot storage directory"
        );
    }

    #[test]
    fn snapshot_config_missing_home_is_not_a_config_fault() {
        let err = AegisError::Snapshot(SnapshotError::Config(
            "HOME is not set; cannot determine snapshot storage directory".to_string(),
        ));
        assert!(
            !err.is_config_fault(),
            "an unset environment variable is not fixed by editing aegis.toml"
        );
    }

    #[test]
    fn insecure_audit_artifact_message_has_no_io_error_prefix() {
        let err = AegisError::Audit(AuditError::InsecureAuditArtifact {
            path: "/home/user/.aegis/audit.jsonl".to_string(),
            detail: "mode 0644, expected 0600".to_string(),
        });
        assert_eq!(
            err.to_string(),
            "audit artifact '/home/user/.aegis/audit.jsonl' is insecure: mode 0644, expected 0600"
        );
    }

    #[test]
    fn rollback_conflict_is_matchable_through_the_snapshot_wrapper() {
        let err = AegisError::Snapshot(SnapshotError::RollbackConflict {
            stash_ref: "stash@{0}".to_string(),
            cwd: "/repo".to_string(),
            details: "conflict".to_string(),
        });
        assert!(matches!(
            err,
            AegisError::Snapshot(SnapshotError::RollbackConflict { .. })
        ));
    }
}
