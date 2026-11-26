use aegis_language::SourceLanguage;
use aegis_types::{
    AnalysisProvenance, DegradationReason, DetectedOperation, HighlightRange, MatchResult,
    OperationKind, language_match,
};

use super::queue::QueueTarget;

#[derive(Debug, Clone)]
pub struct SinkDecision {
    pub sink_match: MatchResult,
    pub recursive_target: Option<QueueTarget>,
    pub degradation: Option<DegradationReason>,
}

pub fn handle_sink(
    op: &DetectedOperation,
    payload_language: SourceLanguage,
    resolved_payload: Option<&str>,
    provenance: AnalysisProvenance,
    highlight_range: Option<HighlightRange>,
    parent_depth: u32,
) -> SinkDecision {
    debug_assert!(
        op.kind == OperationKind::CodeExecution,
        "handle_sink is only for recognized CodeExecution sinks; got {:?}",
        op.kind,
    );

    let sink_match = language_match(op, provenance, highlight_range);

    match resolved_payload {
        Some(literal) => SinkDecision {
            sink_match,
            recursive_target: Some(QueueTarget::new(
                payload_language,
                literal.to_string(),
                parent_depth + 1,
            )),
            degradation: None,
        },
        None => SinkDecision {
            sink_match,
            recursive_target: None,
            degradation: Some(DegradationReason::DynamicSource),
        },
    }
}

#[cfg(test)]
#[path = "recursive_tests.rs"]
mod tests;
