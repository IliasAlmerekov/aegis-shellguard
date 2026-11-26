use aegis_language::SourceLanguage;
use aegis_language::operation::{
    AdapterDegradation, AdapterResult, ByteSpan as LangSpan, DetectedOperation as LangOp,
    OperandCertainty as LangCert, OperationKind as LangKind, OperationModifiers as LangMods,
};
use aegis_types::{
    AnalysisProvenance, AnalysisStatus, ByteSpan, DegradationReason, DetectedOperation,
    LanguageAnalysisResult, OperandCertainty, OperationKind, OperationModifiers, SourceOrigin,
    language_match,
};

use super::queue::QueueTarget;
use super::recursive::handle_sink;

#[derive(Debug, Clone)]
pub struct MappingOutcome {
    pub analysis: LanguageAnalysisResult,
    pub recursive_targets: Vec<QueueTarget>,
}

#[must_use]
pub fn map_operation(op: &LangOp) -> Option<DetectedOperation> {
    Some(DetectedOperation {
        kind: map_kind(op.kind)?,
        modifiers: map_modifiers(op.modifiers),
        certainty: map_certainty(op.certainty),
    })
}

#[must_use]
pub fn map_adapter_result(
    adapter: &AdapterResult,
    language: SourceLanguage,
    source_origin: SourceOrigin,
    file_path: Option<String>,
    source_hash: Option<String>,
    parent_depth: u32,
) -> MappingOutcome {
    let mut matches = Vec::new();
    let mut recursive_targets = Vec::new();
    let mut reasons: Vec<DegradationReason> = Vec::new();
    let provenance_context = ProvenanceContext {
        language,
        source_origin,
        file_path: file_path.as_deref(),
        source_hash: source_hash.as_deref(),
    };

    for op in &adapter.operations {
        let Some(mapped) = map_operation(op) else {
            continue;
        };
        let span = map_span(op.span);

        if op.kind == LangKind::CodeExecution {
            let payload_language = op.payload.as_ref().map_or(language, |p| p.language);
            let resolved_payload = op.payload.as_ref().map(|p| p.source.as_str());

            let (status, degradation_reason) = if op.payload.is_some() {
                (AnalysisStatus::Complete, None)
            } else {
                (
                    AnalysisStatus::Degraded,
                    Some(DegradationReason::DynamicSource),
                )
            };
            let provenance = build_provenance(
                &mapped,
                &provenance_context,
                span,
                status,
                degradation_reason,
            );

            let decision = handle_sink(
                &mapped,
                payload_language,
                resolved_payload,
                provenance,
                None,
                parent_depth,
            );
            matches.push(decision.sink_match);
            if let Some(target) = decision.recursive_target {
                recursive_targets.push(target);
            }
            if let Some(reason) = decision.degradation {
                push_unique(&mut reasons, reason);
            }
        } else {
            let provenance = build_provenance(
                &mapped,
                &provenance_context,
                span,
                AnalysisStatus::Complete,
                None,
            );
            let mr = language_match(&mapped, provenance, None);
            matches.push(mr);
        }
    }

    if adapter.parse_errors > 0 {
        push_unique(&mut reasons, DegradationReason::IncompleteSyntax);
    }
    if matches!(
        adapter.degradation,
        Some(AdapterDegradation::UnsupportedEncoding)
    ) {
        push_unique(&mut reasons, DegradationReason::UnsupportedEncoding);
    }

    let status = if !reasons.is_empty() {
        AnalysisStatus::Degraded
    } else if !matches.is_empty() {
        AnalysisStatus::Complete
    } else {
        AnalysisStatus::NotApplicable
    };

    MappingOutcome {
        analysis: LanguageAnalysisResult {
            status,
            matches,
            degradation_reasons: reasons,
        },
        recursive_targets,
    }
}

struct ProvenanceContext<'a> {
    language: SourceLanguage,
    source_origin: SourceOrigin,
    file_path: Option<&'a str>,
    source_hash: Option<&'a str>,
}

fn build_provenance(
    op: &DetectedOperation,
    context: &ProvenanceContext<'_>,
    span: ByteSpan,
    status: AnalysisStatus,
    degradation_reason: Option<DegradationReason>,
) -> AnalysisProvenance {
    AnalysisProvenance {
        language: Some(context.language.id().to_string()),
        source_origin: context.source_origin,
        rule_id: None,
        operation: Some(op.clone()),
        file_path: context.file_path.map(str::to_string),
        source_hash: context.source_hash.map(str::to_string),
        span: Some(span),
        certainty: op.certainty,
        status,
        degradation_reason,
    }
}

fn map_span(span: LangSpan) -> ByteSpan {
    ByteSpan {
        line: span.line,
        column: span.column,
        byte_start: span.byte_start,
        byte_end: span.byte_end,
    }
}

fn map_kind(kind: LangKind) -> Option<OperationKind> {
    match kind {
        LangKind::FilesystemDelete => Some(OperationKind::FilesystemDelete),
        LangKind::FilesystemOverwrite => Some(OperationKind::FilesystemOverwrite),
        LangKind::PermissionOrOwnershipChange => Some(OperationKind::PermissionOrOwnershipChange),
        LangKind::DeviceOrCriticalWrite => Some(OperationKind::DeviceOrCriticalWrite),
        LangKind::DatabaseDestructive => Some(OperationKind::DatabaseDestructive),
        LangKind::CodeExecution => Some(OperationKind::CodeExecution),
        LangKind::CloudDestructive => Some(OperationKind::CloudDestructive),
        LangKind::ContainerDestructive => Some(OperationKind::ContainerDestructive),
        LangKind::PackageDestructive => Some(OperationKind::PackageDestructive),
        _ => None,
    }
}

fn map_modifiers(mods: LangMods) -> OperationModifiers {
    OperationModifiers {
        recursive: mods.recursive,
        forced: mods.forced,
        destructive_mode: mods.destructive_mode,
    }
}

fn map_certainty(certainty: LangCert) -> OperandCertainty {
    match certainty {
        LangCert::Known => OperandCertainty::Known,
        LangCert::Partial => OperandCertainty::Partial,
        _ => OperandCertainty::Dynamic,
    }
}

fn push_unique(reasons: &mut Vec<DegradationReason>, reason: DegradationReason) {
    if !reasons.contains(&reason) {
        reasons.push(reason);
    }
}
