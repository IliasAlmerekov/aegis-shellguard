use super::{SinkDecision, handle_sink};
use crate::analysis::classifier::tests::provenance;
use crate::analysis::queue::{AnalysisQueue, PushOutcome, QueueBudget, QueueTarget};
use aegis_language::SourceLanguage;
use aegis_types::{
    Category, DegradationReason, DetectedOperation, DetectionMechanism, OperandCertainty,
    OperationKind, OperationModifiers, RiskLevel,
};

fn code_exec(certainty: OperandCertainty) -> DetectedOperation {
    DetectedOperation {
        kind: OperationKind::CodeExecution,
        modifiers: OperationModifiers::default(),
        certainty,
    }
}

#[test]
fn literal_payload_emits_match_and_recursive_target() {
    let op = code_exec(OperandCertainty::Known);
    let d: SinkDecision = handle_sink(
        &op,
        SourceLanguage::Bash,
        Some("rm -rf /tmp/x"),
        provenance(&op),
        None,
        0,
    );

    assert_eq!(d.sink_match.pattern.risk, RiskLevel::Danger);
    assert_eq!(d.sink_match.pattern.category, Category::Process);
    assert_eq!(d.sink_match.pattern.id.as_ref(), "LANG-EXEC");
    assert_eq!(
        d.sink_match.evidence.mechanism(),
        DetectionMechanism::LanguageRule
    );
    let target = d
        .recursive_target
        .expect("literal payload enqueues a target");
    assert_eq!(target.language, SourceLanguage::Bash);
    assert_eq!(
        target.depth, 1,
        "root sink at depth 0 → payload target at depth 1"
    );
    assert_eq!(target.source, "rm -rf /tmp/x");
    assert_eq!(d.degradation, None);
}

#[test]
fn dynamic_payload_emits_match_and_degradation_without_target() {
    let op = code_exec(OperandCertainty::Dynamic);
    let d = handle_sink(&op, SourceLanguage::Bash, None, provenance(&op), None, 0);

    assert_eq!(d.sink_match.pattern.risk, RiskLevel::Danger);
    assert_eq!(d.sink_match.pattern.id.as_ref(), "LANG-EXEC");
    assert!(
        d.recursive_target.is_none(),
        "a dynamic payload must not be enqueued"
    );
    assert_eq!(d.degradation, Some(DegradationReason::DynamicSource));
}

#[test]
fn partial_payload_emits_match_and_degradation_without_target() {
    let op = code_exec(OperandCertainty::Partial);
    let d = handle_sink(&op, SourceLanguage::Bash, None, provenance(&op), None, 0);
    assert_eq!(d.sink_match.pattern.risk, RiskLevel::Danger);
    assert!(d.recursive_target.is_none());
    assert_eq!(d.degradation, Some(DegradationReason::DynamicSource));
}

#[test]
fn decode_to_eval_emits_match_and_degradation_without_decoding() {
    let op = code_exec(OperandCertainty::Dynamic);
    let d = handle_sink(&op, SourceLanguage::Python, None, provenance(&op), None, 0);
    assert_eq!(d.sink_match.pattern.id.as_ref(), "LANG-EXEC");
    assert_eq!(d.sink_match.pattern.risk, RiskLevel::Danger);
    assert!(
        d.recursive_target.is_none(),
        "encoded payload must not be decoded/enqueued"
    );
    assert_eq!(d.degradation, Some(DegradationReason::DynamicSource));
}

#[test]
fn recursive_target_depth_is_parent_depth_plus_one() {
    let op = code_exec(OperandCertainty::Known);
    let d = handle_sink(
        &op,
        SourceLanguage::Bash,
        Some("rm x"),
        provenance(&op),
        None,
        3,
    );
    assert_eq!(d.recursive_target.unwrap().depth, 4);
}

#[test]
fn cross_language_payload_target_uses_payload_language() {
    let op = code_exec(OperandCertainty::Known);
    let d = handle_sink(
        &op,
        SourceLanguage::JavaScript,
        Some("fs.rmSync('x')"),
        provenance(&op),
        None,
        0,
    );
    assert_eq!(
        d.recursive_target.unwrap().language,
        SourceLanguage::JavaScript
    );
}

#[test]
fn sink_match_is_code_execution_for_both_literal_and_dynamic() {
    let lit = handle_sink(
        &code_exec(OperandCertainty::Known),
        SourceLanguage::Bash,
        Some("rm x"),
        provenance(&code_exec(OperandCertainty::Known)),
        None,
        0,
    );
    let dyn_ = handle_sink(
        &code_exec(OperandCertainty::Dynamic),
        SourceLanguage::Bash,
        None,
        provenance(&code_exec(OperandCertainty::Dynamic)),
        None,
        0,
    );
    for (name, d) in [("literal", lit), ("dynamic", dyn_)] {
        assert_eq!(
            d.sink_match.pattern.id.as_ref(),
            "LANG-EXEC",
            "{name} sink must keep the CodeExecution Match",
        );
        assert_eq!(d.sink_match.pattern.risk, RiskLevel::Danger);
    }
}

#[test]
fn degradation_never_lowers_sink_risk() {
    let d = handle_sink(
        &code_exec(OperandCertainty::Dynamic),
        SourceLanguage::Bash,
        None,
        provenance(&code_exec(OperandCertainty::Dynamic)),
        None,
        0,
    );
    assert_eq!(d.degradation, Some(DegradationReason::DynamicSource));
    assert_eq!(d.sink_match.pattern.risk, RiskLevel::Danger);
}

#[test]
fn cross_language_literal_payload_is_accepted_by_the_queue() {
    let op = code_exec(OperandCertainty::Known);
    let d = handle_sink(
        &op,
        SourceLanguage::JavaScript,
        Some("fs.rmSync('x')"),
        provenance(&op),
        None,
        0,
    );
    let target = d.recursive_target.expect("literal payload → target");

    let mut q = AnalysisQueue::new(QueueBudget::L1_DEFAULT);
    q.push(QueueTarget::new(
        SourceLanguage::Python,
        "subprocess.run(['node','-e','fs.rmSync(\\'x\\')'])".to_string(),
        0,
    ));
    let outcome = q.push(target);
    assert_eq!(outcome, PushOutcome::Accepted);
    assert_eq!(q.accepted_count(), 2);
}

#[test]
fn dynamic_payload_target_is_not_pushed_to_queue() {
    let op = code_exec(OperandCertainty::Dynamic);
    let d = handle_sink(&op, SourceLanguage::Bash, None, provenance(&op), None, 0);
    assert!(d.recursive_target.is_none());
    let q = AnalysisQueue::new(QueueBudget::L1_DEFAULT);
    assert_eq!(q.accepted_count(), 0);
}
