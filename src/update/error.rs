#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum UpdateError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HOME is not set; cannot resolve the Aegis update state directory")]
    NoHome,

    #[error("update state at {path} is corrupt: {detail}")]
    CorruptState { path: String, detail: String },

    #[error("registry response could not be parsed: {detail}")]
    MalformedRegistryResponse { detail: String },

    #[error("{version:?} is not a valid SemVer version: {detail}")]
    InvalidVersion { version: String, detail: String },

    #[error("the update check could not reach the registry: {detail}")]
    NetworkUnavailable { detail: String },

    #[error("another update-state operation is in progress")]
    StateBusy,

    #[error("internal error: {detail}")]
    Internal { detail: String },
}
