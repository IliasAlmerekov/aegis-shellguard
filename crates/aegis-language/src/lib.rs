//! Language-aware analysis for Aegis (ADR-022).
//!
//! Owns Tree-sitter parsing, the release grammar manifest, the worker wire
//! protocol, and language adapters that emit language-neutral detected
//! operations. Per ADR-022:
//!
//! - Additive slow path: never replaces the shell `Scanner` or regresses the
//!   no-source hot path (ADR-022 §1).
//! - Ephemeral worker process: no daemon, plugin loader, or network service
//!   (ADR-022 §2).
//! - Sole crate with the pinned Tree-sitter native C toolchain; `aegis-types`
//!   never depends on this crate (ADR-022 §4, §8).
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable
    )
)]

pub mod language;
#[cfg(feature = "grammars")]
pub mod languages;
pub mod manifest;
#[cfg(feature = "grammars")]
pub mod operation;
#[cfg(feature = "grammars")]
pub mod protocol;
pub mod router;
#[cfg(feature = "grammars")]
pub mod worker;

pub use language::SourceLanguage;
#[cfg(feature = "grammars")]
pub use language::{ParseError, parse};
#[cfg(feature = "grammars")]
pub use protocol::{
    DecodeError, DecodedFrame, EncodeError, MAX_FRAME_PAYLOAD, MAX_SOURCE_BYTES, Request, Response,
};
pub use router::SourceTarget;
#[cfg(feature = "grammars")]
pub use worker::{Outcome, RunOutcome, analyze, run};
