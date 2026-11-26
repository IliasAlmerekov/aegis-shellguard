#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ScannerError {
    #[error("invalid pattern {id}: {reason}")]
    InvalidPattern { id: String, reason: String },

    #[error("failed to build scanner: {0}")]
    Build(String),
}
