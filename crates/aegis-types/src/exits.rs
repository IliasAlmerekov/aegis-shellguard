//! Exit-code contract for Aegis-generated outcomes.
//!
//! | Code | Meaning |
//! |------|---------|
//! | 0 | Success. |
//! | 1 | Snapshot absent for `snapshot verify` or `rollback` (ADR-052). |
//! | 2 | Denied or invalid invocation; rollback may refuse an inconclusive check. |
//! | 3 | Blocked. |
//! | 4 | Aegis or configuration error. |
//! | 5 | Snapshot verification was inconclusive and none was absent. |
//!
//! A launched command's exit code is passed through, including these values.

/// The user denied the command at the confirmation dialog.
pub const EXIT_DENIED: i32 = 2;
/// The command matched a `Block`-level pattern and was hard-stopped.
pub const EXIT_BLOCKED: i32 = 3;
/// Aegis or configuration failure prevented execution or validation.
pub const EXIT_INTERNAL: i32 = 4;
/// A Snapshot liveness check found the artifact absent.
pub const EXIT_ABSENT: i32 = 1;
/// Snapshot verification was inconclusive and none was absent.
pub const EXIT_INCONCLUSIVE: i32 = 5;
/// The invocation is invalid.
pub const EXIT_USAGE: i32 = 2;
