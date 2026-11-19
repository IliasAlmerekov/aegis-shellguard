//! Typed error hierarchy for the `aegis-snapshot` crate.

/// Typed error for snapshot creation and rollback operations.
///
/// Aegis-owned failures carry typed variants. [`SnapshotError::Snapshot`] is
/// left for text supplied by a [`crate::SnapshotPlugin`] outside this crate.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SnapshotError {
    /// Snapshot creation or rollback operation failed, as reported by a plugin
    /// outside this crate.
    #[error("snapshot error: {0}")]
    Snapshot(String),

    /// Snapshot identifier has an invalid structure.
    #[error("snapshot error: malformed snapshot_id: {id:?}")]
    MalformedSnapshotId {
        /// Identifier supplied for the operation.
        id: String,
    },

    /// A Snapshot ID component is invalid (encoding, port, dump reference).
    #[error("snapshot error: malformed snapshot_id: invalid {component} {value:?}")]
    InvalidSnapshotIdComponent {
        /// Component description, for example `port` or `dump path encoding`.
        component: String,
        /// Offending component value.
        value: String,
    },

    /// A Snapshot ID path component is not an acceptable path.
    #[error("snapshot error: malformed snapshot_id: invalid {label} {path:?}")]
    InvalidSnapshotIdPath {
        /// Component name.
        label: String,
        /// Decoded path that was rejected.
        path: std::path::PathBuf,
    },

    /// Snapshot ID version is not supported by the parser.
    #[error("snapshot error: unsupported snapshot_id version {version:?}")]
    UnsupportedSnapshotIdVersion {
        /// Version found in the identifier.
        version: String,
    },

    /// Git stash entry for the snapshot no longer exists.
    #[error("snapshot error: stash entry not found for hash {hash}")]
    GitStashNotFound {
        /// Commit hash recorded in the snapshot ID.
        hash: String,
    },

    /// The stash entry vanished while the current work was being parked.
    #[error(
        "snapshot error: stash entry disappeared while parking current work; parked work was retained and rollback stopped for hash {hash}"
    )]
    ParkedStashDisappeared {
        /// Commit hash recorded in the snapshot ID.
        hash: String,
    },

    /// `git stash push` failed while parking the current work before rollback.
    #[error("snapshot error: could not park the current working tree before rollback: {stderr}")]
    ParkWorkingTreeFailed {
        /// Git's standard error output.
        stderr: String,
    },

    /// Current database target differs from the target recorded in the manifest.
    #[error(
        "snapshot error: rollback target mismatch: current config differs from manifest target: {details}"
    )]
    RollbackTargetMismatch {
        /// Differences between the manifest and the current configuration.
        details: String,
    },

    /// Recomputed manifest eligibility does not allow rollback.
    #[error(
        "snapshot error: rollback denied: recomputed manifest eligibility is not allowed (status: {status})"
    )]
    RollbackDenied {
        /// Debug rendering of the recomputed overall status.
        status: String,
    },

    /// No registered plugin can perform the operation.
    #[error("snapshot error: snapshot plugin {plugin:?} is not available for {operation}")]
    PluginUnavailable {
        /// Requested plugin name.
        plugin: String,
        /// Operation that needed the plugin (`rollback` or `delete`).
        operation: &'static str,
    },

    /// Docker snapshot record is not valid JSON or lacks required fields.
    #[error("snapshot error: malformed docker snapshot record: {detail}")]
    MalformedDockerRecord {
        /// JSON parser error details.
        detail: String,
    },

    /// An external command ran and reported a failure.
    #[error("snapshot error: {command} failed: {detail}")]
    CommandFailed {
        /// Command that failed, for example `pg_dump` or `docker run <image>`.
        command: String,
        /// Standard error output or the underlying error text.
        detail: String,
    },

    /// An external command exited unsuccessfully and gave no detail.
    #[error("snapshot error: {command} failed")]
    CommandExitFailed {
        /// Command that failed.
        command: &'static str,
    },

    /// An external command could not be started.
    #[error("snapshot error: {context}: {detail}")]
    CommandLaunchFailed {
        /// What was being launched, already phrased by the caller.
        context: String,
        /// Underlying launch error text.
        detail: String,
    },

    /// An I/O, parse or serialization step failed.
    #[error("snapshot error: failed to {action}: {detail}")]
    OperationFailed {
        /// What was being attempted, for example `read supabase manifest <path>`.
        action: String,
        /// Underlying error text.
        detail: String,
    },

    /// A child process pipe was not available.
    #[error("snapshot error: failed to {action}")]
    StreamUnavailable {
        /// What was being attempted, for example `capture mysqldump stdout`.
        action: &'static str,
    },

    /// A required value or tool is missing.
    #[error("snapshot error: {subject} is required")]
    Required {
        /// The missing value or tool.
        subject: &'static str,
    },

    /// A value or tool is required under a condition.
    #[error("snapshot error: {subject} is required {condition}")]
    RequiredWhen {
        /// The missing value or tool.
        subject: &'static str,
        /// When it is required, for example `for rollback`.
        condition: &'static str,
    },

    /// An input was rejected with a fixed reason.
    #[error("snapshot error: {reason}")]
    Rejected {
        /// Why the input was rejected.
        reason: &'static str,
    },

    /// The manifest's artifact path is not acceptable.
    #[error("snapshot error: manifest artifacts.db.path {reason}: {path}")]
    ManifestArtifactPathRejected {
        /// Why the path was rejected.
        reason: &'static str,
        /// Path recorded in the manifest.
        path: String,
    },

    /// The manifest summary disagrees with the recomputed invariants.
    #[error("snapshot error: manifest summary does not match recomputed rollback invariants: {}", .mismatches.join(", "))]
    ManifestSummaryMismatch {
        /// Names of the mismatching invariants.
        mismatches: Vec<String>,
    },

    /// Removing a failed snapshot bundle left artifacts behind.
    #[error("snapshot error: {}", .failures.join("; "))]
    CleanupIncomplete {
        /// One entry per artifact that could not be removed.
        failures: Vec<String>,
    },

    /// A snapshot failed and cleaning up its leftovers failed too.
    #[error("snapshot error: {original}; cleanup failed: {cleanup}")]
    CleanupFailed {
        /// The failure that triggered cleanup.
        original: Box<SnapshotError>,
        /// The failure of the cleanup itself.
        cleanup: Box<SnapshotError>,
    },

    /// Configuration error that prevents a snapshot registry from being built.
    #[error("snapshot config error: {0}")]
    Config(String),

    /// `git stash pop` conflicted during rollback.
    ///
    /// The stash entry is preserved — git does not drop it on a conflicted pop.
    /// The error message includes exact recovery commands so the user can finish
    /// the restore manually without losing any work.
    #[error(
        "rollback conflict: git stash pop failed for {stash_ref}.\n\
         Your changes are still saved in the stash. To recover manually:\n  \
           1. Resolve conflicts:  git diff\n  \
           2. Stage resolutions:  git add <files>\n  \
           3. Drop the stash:     git stash drop {stash_ref}\n\
         Details: {details}"
    )]
    RollbackConflict {
        /// The stash reference that failed to pop.
        stash_ref: String,
        /// Working directory where the rollback was attempted.
        cwd: String,
        /// Underlying error details.
        details: String,
    },

    /// The git snapshot stash was written, but the working tree it emptied
    /// could not be restored.
    ///
    /// The captured work is intact in the stash entry; the message carries the
    /// command that puts it back.
    #[error(
        "snapshot taken, but the working tree could not be restored afterwards.\n\
         Your uncommitted work is safe in stash entry {stash_hash}. To put it back:\n  \
           git stash apply --index {stash_hash}\n\
         Details: {details}"
    )]
    SnapshotNotRestored {
        /// Commit hash of the stash entry holding the captured work.
        stash_hash: String,
        /// Working directory where the snapshot was taken.
        cwd: String,
        /// Underlying error details.
        details: String,
    },

    /// The git snapshot stash was written, but Git did not return its hash
    /// before the working tree could be restored.
    #[error(
        "snapshot taken, but the working tree could not be restored afterwards. Run `git stash list`, then restore the captured work with `git stash apply --index <ref>`. Details: {details}"
    )]
    SnapshotNotRestoredUnknown {
        /// Working directory where the snapshot was taken.
        cwd: String,
        /// Underlying error details.
        details: String,
    },

    /// The dump file that was recorded at snapshot time no longer exists.
    #[error(
        "rollback failed: dump file not found at '{path}'. The file may have been deleted manually."
    )]
    RollbackDumpNotFound {
        /// Expected path of the missing dump file.
        path: String,
    },

    /// A snapshot artifact is not provably contained by its trusted snapshot store.
    #[error("{plugin} snapshot artifact '{candidate}' escapes snapshot store '{store}'")]
    PathEscapesSnapshotStore {
        /// Plugin that attempted to use the artifact.
        plugin: &'static str,
        /// Trusted snapshot store configured for that plugin.
        store: String,
        /// Untrusted artifact candidate decoded from a snapshot identifier.
        candidate: String,
    },

    /// A snapshot store or artifact could not be made owner-only before a sensitive write.
    #[error("{plugin} snapshot path '{path}' does not meet owner-only permissions: {detail}")]
    InsecureSnapshotPermissions {
        /// Plugin creating the snapshot artifact.
        plugin: String,
        /// Path whose permissions could not be secured.
        path: String,
        /// Specific reason permission hardening failed.
        detail: String,
    },

    /// The persisted rollback artifact no longer matches the manifest checksum.
    #[error(
        "rollback failed: artifact integrity verification failed for '{path}' (expected SHA-256 {expected_sha256}, got {actual_sha256})"
    )]
    RollbackIntegrityCheckFailed {
        /// Path to the artifact that failed verification.
        path: String,
        /// Expected SHA-256 checksum recorded at snapshot time.
        expected_sha256: String,
        /// Actual SHA-256 checksum computed at rollback time.
        actual_sha256: String,
    },

    /// Deleting a snapshot artifact failed.
    #[error("delete failed for {plugin} snapshot {snapshot_id}: {detail}")]
    DeleteFailed {
        /// Provider that failed the deletion.
        plugin: String,
        /// Snapshot identifier that could not be deleted.
        snapshot_id: String,
        /// Underlying error description. Named `detail`, not `source`, because it is text and not an error value.
        detail: String,
    },

    /// Wrapped I/O error from the standard library.
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
