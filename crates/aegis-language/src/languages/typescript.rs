//! TypeScript language adapter (ADR-022 §3, §7).
//!
//! Captures call sites via `queries/typescript/calls.scm` and delegates
//! interpretation to the shared `family` module, emitting language-neutral
//! [`crate::operation::DetectedOperation`]s without assigning `RiskLevel`
//! directly (ADR-022 §3).

use std::cell::RefCell;
use std::sync::LazyLock;

use tree_sitter::{Parser, Query};

use crate::language::SourceLanguage;
use crate::languages::family;
use crate::operation::AdapterResult;

#[expect(
    clippy::expect_used,
    reason = "a bundled query that fails to compile is a build-time bug; panicking at startup is intended"
)]
static CALLS_QUERY: LazyLock<Query> = LazyLock::new(|| {
    Query::new(
        &SourceLanguage::TypeScript.tree_sitter_language(),
        include_str!("../../queries/typescript/calls.scm"),
    )
    .expect("bundled typescript/calls.scm must compile against the pinned grammar")
});

thread_local! {
    #[expect(
        clippy::expect_used,
        reason = "a pinned grammar that is ABI-incompatible is a build-time bug; panicking at startup is intended"
    )]
    static PARSER: RefCell<Parser> = RefCell::new({
        let mut parser = Parser::new();
        parser
            .set_language(&SourceLanguage::TypeScript.tree_sitter_language())
            .expect("pinned typescript grammar is ABI-compatible with the runtime");
        parser
    });
}

/// Analyze TypeScript `source` for destructive effects and execution sinks.
///
/// Pure and in-process: parses `source` with the pinned TypeScript grammar,
/// runs the call-capture query, and interprets each call site in Rust via the
/// shared `family` module. No filesystem access, no subprocess (ADR-022 §2). A
/// nonzero `parse_errors` means the source was malformed; the root mapping
/// records `DegradationReason::IncompleteSyntax`.
#[must_use]
pub fn analyze(source: &str) -> AdapterResult {
    // `parse` returns `None` only on a NULL C result; treat as a malformed
    // (unrecoverable) parse. A real empty program still produces a tree. The
    // parser is held in `PARSER` (per-thread, one-time `set_language`); the
    // borrow is released before any node traversal below.
    let Some(tree) = PARSER.with(|cell| cell.borrow_mut().parse(source.as_bytes(), None)) else {
        return AdapterResult {
            operations: Vec::new(),
            parse_errors: 1,
            degradation: None,
        };
    };

    let parse_errors = u32::from(tree.root_node().has_error());
    let root = tree.root_node();
    let bytes = source.as_bytes();
    let operations = family::collect_operations(root, bytes, &CALLS_QUERY);

    AdapterResult {
        operations,
        parse_errors,
        degradation: None,
    }
}

#[cfg(test)]
#[path = "typescript_tests.rs"]
mod tests;
