//! Per-language adapters (ADR-022 §3, §9).
//!
//! Each adapter uses Tree-sitter queries for structural capture and typed Rust
//! for semantic interpretation, emitting language-neutral
//! [`crate::operation::DetectedOperation`]s without assigning `RiskLevel`
//! directly (ADR-022 §3).

pub mod bash;
pub mod bash_word;
pub(crate) mod family;
pub mod javascript;
pub mod python;
pub mod typescript;
