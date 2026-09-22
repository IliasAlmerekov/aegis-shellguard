//! Typed error for update-check state, registry, and version handling.

/// Everything that can go wrong resolving update state, running a registry
/// check, or comparing versions.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum UpdateError {
    /// Wrapped I/O error from the standard library.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Neither `HOME` nor `USERPROFILE` is set.
    #[error("HOME is not set; cannot resolve the Aegis update state directory")]
    NoHome,

    /// The on-disk update state file exists but could not be parsed.
    #[error("update state at {path} is corrupt: {detail}")]
    CorruptState {
        /// Path to the offending state file.
        path: String,
        /// Parse failure detail.
        detail: String,
    },

    /// The registry response was not the expected shape.
    #[error("registry response could not be parsed: {detail}")]
    MalformedRegistryResponse {
        /// Parse failure detail.
        detail: String,
    },

    /// A version string (installed or from the registry) failed strict
    /// SemVer parsing.
    #[error("{version:?} is not a valid SemVer version: {detail}")]
    InvalidVersion {
        /// The rejected version string.
        version: String,
        /// Parse failure detail.
        detail: String,
    },

    /// The registry could not be reached, or `curl` is unavailable.
    #[error("the update check could not reach the registry: {detail}")]
    NetworkUnavailable {
        /// Failure detail.
        detail: String,
    },

    /// Another update-state operation is in progress.
    #[error("another update-state operation is in progress")]
    StateBusy,

    /// A fault in the update module's own orchestration, not the network or
    /// on-disk state.
    #[error("internal error: {detail}")]
    Internal {
        /// Description of the internal fault.
        detail: String,
    },
}
