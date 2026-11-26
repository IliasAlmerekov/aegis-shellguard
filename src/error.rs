use aegis_audit::error::AuditError;
use aegis_scanner::ScannerError;
use aegis_snapshot::SnapshotError;

use aegis_config::error::ConfigError;

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum AegisError {
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),

    #[error(transparent)]
    Audit(#[from] AuditError),

    #[error(transparent)]
    Scanner(#[from] ScannerError),

    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error("snapshot id {snapshot_id:?} has been pruned and is no longer recoverable.")]
    SnapshotPruned { snapshot_id: String },

    #[error(
        "snapshot id {snapshot_id:?} was not found in the audit log.\n\
         Hint: run `aegis audit --format json` or `aegis audit --last 20` \
         to find a recorded snapshot id, then retry `aegis rollback <snapshot-id>`."
    )]
    SnapshotIdNotFound { snapshot_id: String },

    #[error(
        "prune completed with failures: {failed_count} candidate(s) could not be deleted. \
         {pruned} snapshot(s) were pruned successfully.\n{failed}"
    )]
    PrunePartialFailure {
        pruned: usize,
        failed_count: usize,
        failed: String,
    },

    #[error("internal error: {detail}")]
    Internal { detail: String },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Update(#[from] crate::update::UpdateError),
}

impl AegisError {
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
            detail: "git stash drop failed".to_string(),
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
