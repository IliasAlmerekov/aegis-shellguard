#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SnapshotError {
    #[error("snapshot error: {0}")]
    Snapshot(String),

    #[error("snapshot error: malformed snapshot_id: {id:?}")]
    MalformedSnapshotId { id: String },

    #[error("snapshot error: malformed snapshot_id: invalid {component} {value:?}")]
    InvalidSnapshotIdComponent { component: String, value: String },

    #[error("snapshot error: malformed snapshot_id: invalid {label} {path:?}")]
    InvalidSnapshotIdPath {
        label: String,
        path: std::path::PathBuf,
    },

    #[error("snapshot error: unsupported snapshot_id version {version:?}")]
    UnsupportedSnapshotIdVersion { version: String },

    #[error("snapshot error: stash entry not found for hash {hash}")]
    GitStashNotFound { hash: String },

    #[error(
        "snapshot error: stash entry disappeared while parking current work; parked work was retained and rollback stopped for hash {hash}"
    )]
    ParkedStashDisappeared { hash: String },

    #[error("snapshot error: could not park the current working tree before rollback: {stderr}")]
    ParkWorkingTreeFailed { stderr: String },

    #[error(
        "snapshot error: rollback target mismatch: current config differs from manifest target: {details}"
    )]
    RollbackTargetMismatch { details: String },

    #[error(
        "snapshot error: rollback denied: recomputed manifest eligibility is not allowed (status: {status})"
    )]
    RollbackDenied { status: String },

    #[error("snapshot error: snapshot plugin {plugin:?} is not available for {operation}")]
    PluginUnavailable {
        plugin: String,
        operation: &'static str,
    },

    #[error("snapshot error: malformed docker snapshot record: {detail}")]
    MalformedDockerRecord { detail: String },

    #[error("snapshot error: {command} failed: {detail}")]
    CommandFailed { command: String, detail: String },

    #[error("snapshot error: {command} failed")]
    CommandExitFailed { command: &'static str },

    #[error("snapshot error: {context}: {detail}")]
    CommandLaunchFailed { context: String, detail: String },

    #[error("snapshot error: failed to {action}: {detail}")]
    OperationFailed { action: String, detail: String },

    #[error("snapshot error: failed to {action}")]
    StreamUnavailable { action: &'static str },

    #[error("snapshot error: {subject} is required")]
    Required { subject: &'static str },

    #[error("snapshot error: {subject} is required {condition}")]
    RequiredWhen {
        subject: &'static str,
        condition: &'static str,
    },

    #[error("snapshot error: {reason}")]
    Rejected { reason: &'static str },

    #[error("snapshot error: manifest artifacts.db.path {reason}: {path}")]
    ManifestArtifactPathRejected { reason: &'static str, path: String },

    #[error("snapshot error: manifest summary does not match recomputed rollback invariants: {}", .mismatches.join(", "))]
    ManifestSummaryMismatch { mismatches: Vec<String> },

    #[error("snapshot error: {}", .failures.join("; "))]
    CleanupIncomplete { failures: Vec<String> },

    #[error("snapshot error: {original}; cleanup failed: {cleanup}")]
    CleanupFailed {
        original: Box<SnapshotError>,
        cleanup: Box<SnapshotError>,
    },

    #[error("snapshot config error: {0}")]
    Config(String),

    #[error(
        "rollback conflict: git stash pop failed for {stash_ref}.\n\
         Your changes are still saved in the stash. To recover manually:\n  \
           1. Resolve conflicts:  git diff\n  \
           2. Stage resolutions:  git add <files>\n  \
           3. Drop the stash:     git stash drop {stash_ref}\n\
         Details: {details}"
    )]
    RollbackConflict {
        stash_ref: String,
        cwd: String,
        details: String,
    },

    #[error(
        "snapshot taken, but the working tree could not be restored afterwards.\n\
         Your uncommitted work is safe in stash entry {stash_hash}. To put it back:\n  \
           git stash apply --index {stash_hash}\n\
         Details: {details}"
    )]
    SnapshotNotRestored {
        stash_hash: String,
        cwd: String,
        details: String,
    },

    #[error(
        "snapshot taken, but the working tree could not be restored afterwards. Run `git stash list`, then restore the captured work with `git stash apply --index <ref>`. Details: {details}"
    )]
    SnapshotNotRestoredUnknown { cwd: String, details: String },

    #[error(
        "rollback failed: dump file not found at '{path}'. The file may have been deleted manually."
    )]
    RollbackDumpNotFound { path: String },

    #[error("{plugin} snapshot artifact '{candidate}' escapes snapshot store '{store}'")]
    PathEscapesSnapshotStore {
        plugin: &'static str,
        store: String,
        candidate: String,
    },

    #[error("{plugin} snapshot path '{path}' does not meet owner-only permissions: {detail}")]
    InsecureSnapshotPermissions {
        plugin: String,
        path: String,
        detail: String,
    },

    #[error(
        "rollback failed: artifact integrity verification failed for '{path}' (expected SHA-256 {expected_sha256}, got {actual_sha256})"
    )]
    RollbackIntegrityCheckFailed {
        path: String,
        expected_sha256: String,
        actual_sha256: String,
    },

    #[error("delete failed for {plugin} snapshot {snapshot_id}: {detail}")]
    DeleteFailed {
        plugin: String,
        snapshot_id: String,
        detail: String,
    },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::SnapshotError;

    #[test]
    fn git_recovery_messages_do_not_interpolate_the_working_directory() {
        let cwd = "/tmp/aegis'; rm -rf /".to_string();
        let errors = [
            SnapshotError::RollbackConflict {
                stash_ref: "stash@{0}".to_string(),
                cwd: cwd.clone(),
                details: "merge conflict".to_string(),
            },
            SnapshotError::SnapshotNotRestored {
                stash_hash: "0123456789abcdef".to_string(),
                cwd: cwd.clone(),
                details: "apply failed".to_string(),
            },
            SnapshotError::SnapshotNotRestoredUnknown {
                cwd: cwd.clone(),
                details: "rev-parse failed".to_string(),
            },
        ];

        for error in errors {
            let message = error.to_string();
            assert!(
                !message.contains(&cwd),
                "recovery message leaked cwd: {message}"
            );
            assert!(message.contains("git stash"));
        }
    }

    #[test]
    fn grouped_variants_render_the_messages_of_the_strings_they_replace() {
        let s = |v: &str| v.to_string();
        let cases: Vec<(SnapshotError, &str)> = vec![
            (
                SnapshotError::CommandFailed {
                    command: s("pg_dump"),
                    detail: s("boom"),
                },
                "snapshot error: pg_dump failed: boom",
            ),
            (
                SnapshotError::CommandExitFailed {
                    command: "git status --porcelain",
                },
                "snapshot error: git status --porcelain failed",
            ),
            (
                SnapshotError::CommandLaunchFailed {
                    context: s("failed to run mysql"),
                    detail: s("not found"),
                },
                "snapshot error: failed to run mysql: not found",
            ),
            (
                SnapshotError::OperationFailed {
                    action: s("read supabase manifest /m.json"),
                    detail: s("denied"),
                },
                "snapshot error: failed to read supabase manifest /m.json: denied",
            ),
            (
                SnapshotError::StreamUnavailable {
                    action: "capture mysqldump stdout",
                },
                "snapshot error: failed to capture mysqldump stdout",
            ),
            (
                SnapshotError::InvalidSnapshotIdComponent {
                    component: s("port"),
                    value: s("7\"x"),
                },
                "snapshot error: malformed snapshot_id: invalid port \"7\\\"x\"",
            ),
            (
                SnapshotError::PluginUnavailable {
                    plugin: s("git"),
                    operation: "delete",
                },
                "snapshot error: snapshot plugin \"git\" is not available for delete",
            ),
            (
                SnapshotError::Required {
                    subject: "manifest bundle root",
                },
                "snapshot error: manifest bundle root is required",
            ),
            (
                SnapshotError::RequiredWhen {
                    subject: "manifest target.db",
                    condition: "for v1",
                },
                "snapshot error: manifest target.db is required for v1",
            ),
            (
                SnapshotError::Rejected {
                    reason: "manifest provider must be supabase",
                },
                "snapshot error: manifest provider must be supabase",
            ),
            (
                SnapshotError::ManifestArtifactPathRejected {
                    reason: "escapes bundle root",
                    path: s("../x"),
                },
                "snapshot error: manifest artifacts.db.path escapes bundle root: ../x",
            ),
            (
                SnapshotError::ManifestSummaryMismatch {
                    mismatches: vec![s("a"), s("b")],
                },
                "snapshot error: manifest summary does not match recomputed rollback invariants: a, b",
            ),
            (
                SnapshotError::RollbackDenied {
                    status: s("Blocked"),
                },
                "snapshot error: rollback denied: recomputed manifest eligibility is not allowed (status: Blocked)",
            ),
            (
                SnapshotError::CleanupFailed {
                    original: Box::new(SnapshotError::Rejected { reason: "x" }),
                    cleanup: Box::new(SnapshotError::CleanupIncomplete {
                        failures: vec![s("p"), s("q")],
                    }),
                },
                "snapshot error: snapshot error: x; cleanup failed: snapshot error: p; q",
            ),
            (
                SnapshotError::ParkWorkingTreeFailed { stderr: s("e\nf") },
                "snapshot error: could not park the current working tree before rollback: e\nf",
            ),
            (
                SnapshotError::ParkedStashDisappeared { hash: s("abc") },
                "snapshot error: stash entry disappeared while parking current work; parked work was retained and rollback stopped for hash abc",
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(error.to_string(), expected);
        }
    }

    #[test]
    fn io_error_keeps_message_and_source() {
        use std::error::Error;
        let error = SnapshotError::from(std::io::Error::other("disk"));
        assert_eq!(error.to_string(), "io error: disk");
        assert!(error.source().is_some());
    }

    #[test]
    fn delete_failed_has_no_error_source() {
        use std::error::Error;
        let error = SnapshotError::DeleteFailed {
            plugin: "git".to_string(),
            snapshot_id: "id".to_string(),
            detail: "why".to_string(),
        };
        assert_eq!(error.to_string(), "delete failed for git snapshot id: why");
        assert!(error.source().is_none());
    }

    #[test]
    fn cleanup_failed_has_no_error_source() {
        use std::error::Error;
        let error = SnapshotError::CleanupFailed {
            original: Box::new(SnapshotError::Rejected { reason: "x" }),
            cleanup: Box::new(SnapshotError::Io(std::io::Error::other("disk"))),
        };
        assert!(error.source().is_none());
    }

    #[test]
    fn test_delete_failed_variant_preserves_context() {
        let err = SnapshotError::DeleteFailed {
            plugin: "git".to_string(),
            snapshot_id: "snap-abc123".to_string(),
            detail: "git stash drop failed".to_string(),
        };
        let message = err.to_string();
        assert!(
            message.contains("git"),
            "DeleteFailed message must name the plugin: {message}"
        );
        assert!(
            message.contains("snap-abc123"),
            "DeleteFailed message must name the snapshot id: {message}"
        );
        assert!(
            message.contains("git stash drop failed"),
            "DeleteFailed message must include the detail: {message}"
        );
    }
}
