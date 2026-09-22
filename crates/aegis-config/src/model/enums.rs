use serde::{Deserialize, Serialize};

/// Audit log integrity protection mode.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema,
)]
#[serde(rename_all = "PascalCase")]
pub enum AuditIntegrityMode {
    /// No integrity chaining.
    Off,
    /// Chained SHA-256 integrity check (default).
    #[default]
    ChainSha256,
}
