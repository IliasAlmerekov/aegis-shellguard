// Shared corpus-test harness for language adapters (ADR-022 §11).
// Lives in `tests/common/` so Cargo does not compile it as a standalone test target.

use aegis_language::SourceLanguage;
use aegis_language::operation::{
    AdapterResult, OperandCertainty, OperationKind, OperationModifiers,
};

/// One expected [`aegis_language::operation::DetectedOperation`]; spans are pinned by unit tests.
#[derive(Debug, Clone, Copy)]
pub struct ExpectedOp {
    pub kind: OperationKind,
    pub modifiers: OperationModifiers,
    pub certainty: OperandCertainty,
    /// For a `CodeExecution` sink: the nested payload's language and recovered
    /// literal source. `None` for non-execution ops and for dynamic payloads.
    pub payload: Option<(SourceLanguage, &'static str)>,
}

/// Assert `analyze(source)` parses cleanly and yields exactly `expected`, in
/// source order, matching each operation's kind, modifiers, certainty, and (for
/// execution sinks) the nested payload `(language, source)`. The caller passes
/// its adapter's `analyze` seam so this helper stays adapter-agnostic.
pub fn assert_ops<F: Fn(&str) -> AdapterResult>(analyze: F, source: &str, expected: &[ExpectedOp]) {
    let result = analyze(source);
    assert_eq!(
        result.parse_errors, 0,
        "corpus source must parse cleanly:\n{source}",
    );
    assert_eq!(
        result.operations.len(),
        expected.len(),
        "operation count mismatch for source:\n{source}\ngot {:?}",
        result.operations,
    );
    for (i, (op, exp)) in result.operations.iter().zip(expected.iter()).enumerate() {
        assert_eq!(op.kind, exp.kind, "op[{i}] kind for source:\n{source}");
        assert_eq!(
            op.modifiers, exp.modifiers,
            "op[{i}] modifiers for source:\n{source}"
        );
        assert_eq!(
            op.certainty, exp.certainty,
            "op[{i}] certainty for source:\n{source}"
        );
        match (op.payload.as_ref(), exp.payload) {
            (None, None) => {}
            (Some(p), Some((lang, src))) => {
                assert_eq!(
                    p.language, lang,
                    "op[{i}] payload language for source:\n{source}"
                );
                assert_eq!(
                    p.source, src,
                    "op[{i}] payload source for source:\n{source}"
                );
            }
            (Some(p), None) => panic!("op[{i}] unexpected payload {p:?} for source:\n{source}"),
            (None, Some(_)) => panic!("op[{i}] missing expected payload for source:\n{source}"),
        }
    }
}

/// Assert a corpus source parses cleanly and yields no operations.
pub fn assert_clean_no_ops<F: Fn(&str) -> AdapterResult>(analyze: F, source: &str) {
    let result = analyze(source);
    assert_eq!(result.parse_errors, 0, "expected clean parse:\n{source}");
    assert!(
        result.operations.is_empty(),
        "expected no operations for source:\n{source}\ngot {:?}",
        result.operations,
    );
}

/// Assert a corpus source is reported as malformed (nonzero parse errors).
pub fn assert_malformed<F: Fn(&str) -> AdapterResult>(analyze: F, source: &str) {
    let result = analyze(source);
    assert!(
        result.parse_errors > 0,
        "expected nonzero parse errors for source:\n{source}",
    );
}

/// Build a `CodeExecution` expectation carrying a Bash literal payload.
pub fn bash_exec(payload: &'static str) -> ExpectedOp {
    ExpectedOp {
        kind: OperationKind::CodeExecution,
        modifiers: OperationModifiers::default(),
        certainty: OperandCertainty::Known,
        payload: Some((SourceLanguage::Bash, payload)),
    }
}
