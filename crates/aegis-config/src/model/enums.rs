use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema,
)]
#[serde(rename_all = "PascalCase")]
#[schemars(description = "Audit log integrity protection mode.")]
pub enum AuditIntegrityMode {
    #[schemars(description = "No integrity chaining.")]
    Off,
    #[schemars(description = "Chained SHA-256 integrity check (default).")]
    #[default]
    ChainSha256,
}
