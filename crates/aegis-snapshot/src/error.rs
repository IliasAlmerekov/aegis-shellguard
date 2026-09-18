//! Typed error hierarchy for the `aegis-snapshot` crate.

/// Typed error for snapshot creation and rollback operations.
#[derive(Debug)]
#[non_exhaustive]
pub enum SnapshotError {
    /// Snapshot creation or rollback operation failed.
    Snapshot(String),

    /// Configuration error that prevents a snapshot registry from being built.
    Config(String),

    /// `git stash pop` conflicted during rollback.
    ///
    /// The stash entry is preserved — git does not drop it on a conflicted pop.
    /// The error message includes exact recovery commands so the user can finish
    /// the restore manually without losing any work.
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
    SnapshotNotRestoredUnknown {
        /// Working directory where the snapshot was taken.
        cwd: String,
        /// Underlying error details.
        details: String,
    },

    /// The dump file that was recorded at snapshot time no longer exists.
    RollbackDumpNotFound {
        /// Expected path of the missing dump file.
        path: String,
    },

    /// A snapshot artifact is not provably contained by its trusted snapshot store.
    PathEscapesSnapshotStore {
        /// Plugin that attempted to use the artifact.
        plugin: &'static str,
        /// Trusted snapshot store configured for that plugin.
        store: String,
        /// Untrusted artifact candidate decoded from a snapshot identifier.
        candidate: String,
    },

    /// A snapshot store or artifact could not be made owner-only before a sensitive write.
    InsecureSnapshotPermissions {
        /// Plugin creating the snapshot artifact.
        plugin: String,
        /// Path whose permissions could not be secured.
        path: String,
        /// Specific reason permission hardening failed.
        detail: String,
    },

    /// The persisted rollback artifact no longer matches the manifest checksum.
    RollbackIntegrityCheckFailed {
        /// Path to the artifact that failed verification.
        path: String,
        /// Expected SHA-256 checksum recorded at snapshot time.
        expected_sha256: String,
        /// Actual SHA-256 checksum computed at rollback time.
        actual_sha256: String,
    },

    /// Deleting a snapshot artifact failed.
    DeleteFailed {
        /// Provider that failed the deletion.
        plugin: String,
        /// Snapshot identifier that could not be deleted.
        snapshot_id: String,
        /// Underlying error description.
        source: String,
    },

    /// Wrapped I/O error from the standard library.
    Io(std::io::Error),
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Snapshot(message) => write!(f, "snapshot error: {message}"),
            Self::Config(message) => write!(f, "snapshot config error: {message}"),
            Self::RollbackConflict {
                stash_ref, details, ..
            } => write!(
                f,
                "rollback conflict: git stash pop failed for {stash_ref}.\n\
                 Your changes are still saved in the stash. To recover manually:\n  \
                   1. Resolve conflicts:  git diff\n  \
                   2. Stage resolutions:  git add <files>\n  \
                   3. Drop the stash:     git stash drop {stash_ref}\n\
                 Details: {details}"
            ),
            Self::SnapshotNotRestored {
                stash_hash,
                details,
                ..
            } => write!(
                f,
                "snapshot taken, but the working tree could not be restored afterwards.\n\
                 Your uncommitted work is safe in stash entry {stash_hash}. To put it back:\n  \
                   git stash apply --index {stash_hash}\n\
                 Details: {details}"
            ),
            Self::SnapshotNotRestoredUnknown { details, .. } => write!(
                f,
                "snapshot taken, but the working tree could not be restored afterwards. Run `git stash list`, then restore the captured work with `git stash apply --index <ref>`. Details: {details}"
            ),
            Self::RollbackDumpNotFound { path } => write!(
                f,
                "rollback failed: dump file not found at '{path}'. The file may have been deleted manually."
            ),
            Self::PathEscapesSnapshotStore {
                plugin,
                store,
                candidate,
            } => write!(
                f,
                "{plugin} snapshot artifact '{candidate}' escapes snapshot store '{store}'"
            ),
            Self::InsecureSnapshotPermissions {
                plugin,
                path,
                detail,
            } => write!(
                f,
                "{plugin} snapshot path '{path}' does not meet owner-only permissions: {detail}"
            ),
            Self::RollbackIntegrityCheckFailed {
                path,
                expected_sha256,
                actual_sha256,
            } => write!(
                f,
                "rollback failed: artifact integrity verification failed for '{path}' (expected SHA-256 {expected_sha256}, got {actual_sha256})"
            ),
            Self::DeleteFailed {
                plugin,
                snapshot_id,
                source,
            } => write!(
                f,
                "delete failed for {plugin} snapshot {snapshot_id}: {source}"
            ),
            Self::Io(error) => write!(f, "io error: {error}"),
        }
    }
}

impl std::error::Error for SnapshotError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for SnapshotError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
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
    fn test_delete_failed_variant_preserves_context() {
        let err = SnapshotError::DeleteFailed {
            plugin: "git".to_string(),
            snapshot_id: "snap-abc123".to_string(),
            source: "git stash drop failed".to_string(),
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
            "DeleteFailed message must include the source error: {message}"
        );
    }
}
