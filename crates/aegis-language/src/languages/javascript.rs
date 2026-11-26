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
        &SourceLanguage::JavaScript.tree_sitter_language(),
        include_str!("../../queries/javascript/calls.scm"),
    )
    .expect("bundled javascript/calls.scm must compile against the pinned grammar")
});

thread_local! {
    #[expect(
        clippy::expect_used,
        reason = "a pinned grammar that is ABI-incompatible is a build-time bug; panicking at startup is intended"
    )]
    static PARSER: RefCell<Parser> = RefCell::new({
        let mut parser = Parser::new();
        parser
            .set_language(&SourceLanguage::JavaScript.tree_sitter_language())
            .expect("pinned javascript grammar is ABI-compatible with the runtime");
        parser
    });
}

#[must_use]
pub fn analyze(source: &str) -> AdapterResult {
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
#[path = "javascript_tests.rs"]
mod tests;
