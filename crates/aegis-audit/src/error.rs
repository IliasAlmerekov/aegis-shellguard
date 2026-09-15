//! Typed error for all audit I/O and serialization operations.

/// Error returned by [`crate::AuditLogger`] operations.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum AuditError {
    /// An audit artifact could not be validated or made owner-only.
    #[error("audit artifact '{path}' is insecure: {detail}")]
    InsecureAuditArtifact {
        /// Path whose filesystem policy check failed.
        path: String,
        /// Specific reason the artifact was rejected.
        detail: String,
    },
    /// Wrapped I/O error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// A line in the persisted audit log could not be parsed as a valid entry.
    #[error("audit error: {}", describe_parse_failure(path, line, source))]
    Parse {
        /// Audit log file that contains the unparsable line.
        path: String,
        /// Line number within the file, when the caller knows it.
        line: Option<u64>,
        /// Underlying JSON parse failure.
        #[source]
        source: serde_json::Error,
    },
    /// An in-memory audit payload could not be serialized to JSON.
    #[error("audit error: failed to serialize audit integrity payload: {source}")]
    Serialize {
        /// Underlying JSON serialization failure.
        #[source]
        source: serde_json::Error,
    },
}

fn describe_parse_failure(path: &str, line: &Option<u64>, source: &serde_json::Error) -> String {
    match line {
        Some(number) => format!("failed to parse audit log line {number} in {path}: {source}"),
        None => format!("failed to parse audit log while scanning tail of {path}: {source}"),
    }
}
