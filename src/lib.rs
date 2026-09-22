#![deny(missing_docs)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

//! Aegis — a lightweight Rust CLI that acts as a `$SHELL` proxy,
//! intercepting AI agent commands and requiring human confirmation before
//! destructive operations.

/// Parent-side language analysis orchestration (worker client).
pub mod analysis;
/// Typed error hierarchy for the whole crate.
pub mod error;
/// Human-readable explanation generation for decisions.
pub mod explanation;
/// Orchestration layer that wraps the pure policy engine.
pub mod planning;
/// Runtime context and dependency wiring.
pub mod runtime;
/// Shared CI detection used by CLI entrypoints.
pub mod runtime_gate;
/// Global on/off toggle state helpers.
pub mod toggle;
/// Watch-mode NDJSON protocol and runner.
pub mod watch;
