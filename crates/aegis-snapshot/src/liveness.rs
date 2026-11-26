use std::io::ErrorKind;
use std::path::Path;
use std::time::Duration;

use serde::Serialize;

pub(crate) const LIVENESS_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Liveness {
    Present,
    Absent { reason: String },
    Inconclusive { reason: String },
    NothingCaptured,
}

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
