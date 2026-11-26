#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum AuditError {
    #[error("audit artifact '{path}' is insecure: {detail}")]
    InsecureAuditArtifact { path: String, detail: String },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error(
        "audit log at '{path}' could not be written: {source} (set AEGIS_AUDIT_PATH to point at a writable location)"
    )]
    WriteFailed {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("audit error: {}", describe_parse_failure(path, line, source))]
    Parse {
        path: String,
        line: Option<u64>,
        #[source]
        source: serde_json::Error,
    },
    #[error("audit error: failed to serialize audit integrity payload: {source}")]
    Serialize {
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
