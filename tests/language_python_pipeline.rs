#![cfg(feature = "language-analysis")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail loudly"
)]

mod support;

use aegis::analysis::mapping::{map_adapter_result, map_operation};
use aegis::analysis::queue::{AnalysisQueue, PushOutcome, QueueBudget, QueueTarget};
use aegis_language::SourceLanguage;
use aegis_language::languages::python::analyze;
use aegis_language::operation::{
    AdapterDegradation, AdapterResult, ByteSpan as LangSpan, DetectedOperation as LangOp,
    OperandCertainty as LangCert, OperationKind as LangKind, OperationModifiers as LangMods,
};
use aegis_types::{
    AnalysisStatus, Category, DegradationReason, DetectionMechanism, OperandCertainty as TypesCert,
    OperationKind as TypesKind, RiskLevel, SourceOrigin,
};

fn map_python(source: &str) -> aegis::analysis::mapping::MappingOutcome {
    let adapter = analyze(source);
    map_adapter_result(
        &adapter,
        SourceLanguage::Python,
        SourceOrigin::Inline,
        None,
        None,
        0,
    )
}

#[test]
fn every_operation_kind_maps_one_for_one() {
    let cases: [(LangKind, TypesKind); 9] = [
        (LangKind::FilesystemDelete, TypesKind::FilesystemDelete),
        (
            LangKind::FilesystemOverwrite,
            TypesKind::FilesystemOverwrite,
        ),
        (
            LangKind::PermissionOrOwnershipChange,
            TypesKind::PermissionOrOwnershipChange,
        ),
        (
            LangKind::DeviceOrCriticalWrite,
            TypesKind::DeviceOrCriticalWrite,
        ),
        (
            LangKind::DatabaseDestructive,
            TypesKind::DatabaseDestructive,
        ),
        (LangKind::CodeExecution, TypesKind::CodeExecution),
        (LangKind::CloudDestructive, TypesKind::CloudDestructive),
        (
            LangKind::ContainerDestructive,
            TypesKind::ContainerDestructive,
        ),
        (LangKind::PackageDestructive, TypesKind::PackageDestructive),
    ];
    for (lang_kind, expected) in cases {
        let lang_op = LangOp {
            kind: lang_kind,
            modifiers: LangMods {
                recursive: true,
                forced: true,
                destructive_mode: true,
            },
            certainty: LangCert::Known,
            span: LangSpan {
                line: 1,
                column: 1,
                byte_start: 0,
                byte_end: 4,
            },
            payload: None,
        };
        let mapped = map_operation(&lang_op).expect("every known kind maps to Some");
        assert_eq!(
            mapped.kind, expected,
            "OperationKind {:?} must map one-for-one",
            lang_kind,
        );
        assert!(
            mapped.modifiers.recursive,
            "{:?}: recursive preserved",
            lang_kind
        );
        assert!(mapped.modifiers.forced, "{:?}: forced preserved", lang_kind);
        assert!(
            mapped.modifiers.destructive_mode,
            "{:?}: destructive_mode preserved",
            lang_kind,
        );
        assert_eq!(
            mapped.certainty,
            TypesCert::Known,
            "{:?}: certainty preserved",
            lang_kind,
        );
    }
}

#[test]
fn known_exec_payload_emits_match_and_cross_language_recursive_target() {
    let outcome = map_python("os.system(\"rm -rf /tmp/x\")");

    let exec = outcome
        .analysis
        .matches
        .iter()
        .find(|m| m.pattern.id.as_ref() == "LANG-EXEC")
        .expect("a CodeExecution sink emits a LANG-EXEC Match");
    assert_eq!(exec.pattern.risk, RiskLevel::Danger);
    assert_eq!(exec.pattern.category, Category::Process);
    assert_eq!(exec.evidence.mechanism(), DetectionMechanism::LanguageRule,);

    let target = outcome
        .recursive_targets
        .first()
        .expect("a literal payload enqueues a recursive target");
    assert_eq!(
        target.language,
        SourceLanguage::Bash,
        "cross-language payload"
    );
    assert_eq!(
        target.depth, 1,
        "root sink at depth 0 → payload target at depth 1"
    );
    assert_eq!(target.source, "rm -rf /tmp/x");

    assert!(
        outcome.analysis.degradation_reasons.is_empty(),
        "a literal payload records no degradation",
    );
    assert_eq!(outcome.analysis.status, AnalysisStatus::Complete);
}

#[test]
fn dynamic_exec_payload_emits_match_and_degradation_without_target() {
    let outcome = map_python("subprocess.run(cmd)");

    assert_eq!(
        outcome
            .analysis
            .matches
            .iter()
            .filter(|m| m.pattern.id.as_ref() == "LANG-EXEC")
            .count(),
        1,
        "a dynamic sink keeps its CodeExecution Match",
    );
    assert!(
        outcome.recursive_targets.is_empty(),
        "a dynamic payload must not be enqueued",
    );
    assert!(
        outcome
            .analysis
            .degradation_reasons
            .contains(&DegradationReason::DynamicSource),
        "a dynamic payload records DynamicSource degradation",
    );
    assert_eq!(outcome.analysis.status, AnalysisStatus::Degraded);
}

#[test]
fn filesystem_delete_emits_match_without_target_or_degradation() {
    let outcome = map_python("os.remove('x')");

    let del = outcome
        .analysis
        .matches
        .iter()
        .find(|m| m.pattern.id.as_ref() == "LANG-FS-DEL")
        .expect("os.remove emits a filesystem-delete Match");
    assert_eq!(del.pattern.risk, RiskLevel::Warn);
    assert_eq!(del.pattern.category, Category::Filesystem);
    assert!(
        outcome.recursive_targets.is_empty(),
        "a non-execution op enqueues no target",
    );
    assert!(
        outcome.analysis.degradation_reasons.is_empty(),
        "a Known non-execution op records no degradation",
    );
    assert_eq!(outcome.analysis.status, AnalysisStatus::Complete);
}

#[test]
fn parse_errors_record_incomplete_syntax_degradation() {
    let op = LangOp {
        kind: LangKind::FilesystemDelete,
        modifiers: LangMods::default(),
        certainty: LangCert::Known,
        span: LangSpan {
            line: 1,
            column: 1,
            byte_start: 0,
            byte_end: 13,
        },
        payload: None,
    };
    let adapter = AdapterResult {
        operations: vec![op],
        parse_errors: 1,
        degradation: None,
    };
    let outcome = map_adapter_result(
        &adapter,
        SourceLanguage::Python,
        SourceOrigin::Inline,
        None,
        None,
        0,
    );

    assert!(
        outcome
            .analysis
            .degradation_reasons
            .contains(&DegradationReason::IncompleteSyntax),
        "parse errors record IncompleteSyntax degradation",
    );
    assert_eq!(outcome.analysis.status, AnalysisStatus::Degraded);
    assert!(
        outcome
            .analysis
            .matches
            .iter()
            .any(|m| m.pattern.id.as_ref() == "LANG-FS-DEL"),
        "the detected op's Match is retained despite the parse error",
    );
}

#[test]
fn adapter_unsupported_encoding_records_unsupported_encoding_degradation() {
    let adapter = AdapterResult {
        operations: Vec::new(),
        parse_errors: 0,
        degradation: Some(AdapterDegradation::UnsupportedEncoding),
    };
    let outcome = map_adapter_result(
        &adapter,
        SourceLanguage::Bash,
        SourceOrigin::Inline,
        None,
        None,
        0,
    );

    assert_eq!(outcome.analysis.status, AnalysisStatus::Degraded);
    assert_eq!(
        outcome.analysis.degradation_reasons,
        vec![DegradationReason::UnsupportedEncoding]
    );
}

#[test]
fn empty_adapter_result_is_not_applicable() {
    let outcome = map_adapter_result(
        &AdapterResult::default(),
        SourceLanguage::Python,
        SourceOrigin::Inline,
        None,
        None,
        0,
    );
    assert_eq!(outcome.analysis.status, AnalysisStatus::NotApplicable);
    assert!(outcome.analysis.matches.is_empty());
    assert!(outcome.recursive_targets.is_empty());
    assert!(outcome.analysis.degradation_reasons.is_empty());
}

#[test]
fn recursive_target_is_accepted_by_parent_queue() {
    let outcome = map_python("os.system(\"rm -rf /tmp/x\")");
    let target = outcome
        .recursive_targets
        .into_iter()
        .next()
        .expect("literal payload → recursive target");

    let mut q = AnalysisQueue::new(QueueBudget::L1_DEFAULT);
    q.push(QueueTarget::new(
        SourceLanguage::Python,
        "os.system(\"rm -rf /tmp/x\")".to_string(),
        0,
    ));
    let outcome = q.push(target);
    assert_eq!(outcome, PushOutcome::Accepted);
    assert_eq!(q.accepted_count(), 2);
}

#[test]
fn non_exec_dynamic_operand_emits_match_without_degradation() {
    let outcome = map_python("os.remove(path)");

    assert!(
        outcome
            .analysis
            .matches
            .iter()
            .any(|m| m.pattern.id.as_ref() == "LANG-FS-DEL"),
        "a dynamic-operand filesystem delete still emits its Match",
    );
    assert!(
        !outcome
            .analysis
            .degradation_reasons
            .contains(&DegradationReason::DynamicSource),
        "a non-execution dynamic operand must NOT record DynamicSource",
    );
    assert!(
        outcome.analysis.degradation_reasons.is_empty(),
        "a non-execution dynamic operand records no degradation",
    );
    assert_eq!(outcome.analysis.status, AnalysisStatus::Complete);
    assert!(
        outcome.recursive_targets.is_empty(),
        "a non-execution op enqueues no recursive target",
    );
}
