//! Snapshot liveness: whether a recorded Snapshot can still be restored now
//! (ADR-052).

use std::io::ErrorKind;
use std::path::Path;
use std::time::Duration;

use serde::Serialize;

/// Upper bound for one liveness check; a check that overruns is
/// [`Liveness::Inconclusive`] (ADR-052 §3).
pub(crate) const LIVENESS_TIMEOUT: Duration = Duration::from_secs(5);

/// The outcome of one Snapshot liveness check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Liveness {
    /// The artifact Rollback needs is there.
    Present,
    /// The artifact is confirmed gone.
    Absent {
        /// What is missing.
        reason: String,
    },
    /// The check could not finish, so the artifact's state is unknown.
    Inconclusive {
        /// Why the check could not finish.
        reason: String,
    },
    /// The Snapshot recorded that there was nothing to capture.
    NothingCaptured,
}

/// Liveness of a local dump file: no network, one `stat`.
pub(crate) async fn local_file_liveness(path: &Path) -> Liveness {
    match tokio::fs::metadata(path).await {
        Ok(_) => Liveness::Present,
        Err(error) if error.kind() == ErrorKind::NotFound => Liveness::Absent {
            reason: format!("artifact {} no longer exists", path.display()),
        },
        Err(error) => Liveness::Inconclusive {
            reason: format!("could not stat artifact {}: {error}", path.display()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_file_liveness_is_inconclusive_on_a_stat_error_other_than_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file");
        std::fs::write(&file, b"x").unwrap();

        // A path below a regular file fails with ENOTDIR, not NotFound.
        let outcome = local_file_liveness(&file.join("dump")).await;

        assert!(
            matches!(outcome, Liveness::Inconclusive { .. }),
            "{outcome:?}"
        );
    }

    #[test]
    fn liveness_serializes_with_a_tagged_outcome() {
        let json = serde_json::to_string(&Liveness::Absent {
            reason: "gone".to_string(),
        })
        .unwrap();

        assert_eq!(json, r#"{"outcome":"absent","reason":"gone"}"#);
    }
}
