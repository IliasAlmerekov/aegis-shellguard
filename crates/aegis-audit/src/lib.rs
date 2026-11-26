#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable
    )
)]

pub mod error;
pub mod ledger;
pub mod logger;
mod secure_fs;
pub use error::AuditError;
pub use ledger::{CommandSnapshot, RecordedCommand, SnapshotLedger, SnapshotLookupError};
pub use logger::{
    AuditEntry, AuditIntegrityReport, AuditIntegrityStatus, AuditLogger, AuditQuery,
    AuditRotationPolicy, AuditSnapshot, AuditSummary, AuditTimestamp, DecisionEntry,
    MatchedPattern, WatchEntry,
};
