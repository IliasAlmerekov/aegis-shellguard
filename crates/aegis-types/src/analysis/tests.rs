use super::*;

#[test]
fn operand_certainty_orders_known_partial_dynamic() {
    assert!(OperandCertainty::Known < OperandCertainty::Partial);
    assert!(OperandCertainty::Partial < OperandCertainty::Dynamic);
    assert_eq!(
        *[
            OperandCertainty::Dynamic,
            OperandCertainty::Known,
            OperandCertainty::Partial,
        ]
        .iter()
        .max()
        .unwrap(),
        OperandCertainty::Dynamic,
    );
}

#[test]
fn analysis_status_orders_not_applicable_complete_degraded() {
    assert!(AnalysisStatus::NotApplicable < AnalysisStatus::Complete);
    assert!(AnalysisStatus::Complete < AnalysisStatus::Degraded);
    assert_eq!(
        *[
            AnalysisStatus::Degraded,
            AnalysisStatus::NotApplicable,
            AnalysisStatus::Complete,
        ]
        .iter()
        .max()
        .unwrap(),
        AnalysisStatus::Degraded,
    );
}

#[test]
fn detection_mechanism_round_trips_through_serde() {
    for variant in [
        DetectionMechanism::RegexPattern,
        DetectionMechanism::TokenPrefixRule,
        DetectionMechanism::LanguageRule,
    ] {
        let json = serde_json::to_string(&variant).unwrap();
        let back: DetectionMechanism = serde_json::from_str(&json).unwrap();
        assert_eq!(back, variant);
    }
    assert_eq!(
        serde_json::to_string(&DetectionMechanism::TokenPrefixRule).unwrap(),
        "\"token_prefix_rule\"",
    );
}

#[test]
fn detection_source_round_trips_through_serde() {
    assert_eq!(
        serde_json::to_string(&DetectionSource::Builtin).unwrap(),
        "\"builtin\"",
    );
    assert_eq!(
        serde_json::to_string(&DetectionSource::Custom).unwrap(),
        "\"custom\"",
    );
    for variant in [DetectionSource::Builtin, DetectionSource::Custom] {
        let json = serde_json::to_string(&variant).unwrap();
        let back: DetectionSource = serde_json::from_str(&json).unwrap();
        assert_eq!(back, variant);
    }
}

#[test]
fn degradation_reason_round_trips_each_bucket() {
    let all = [
        DegradationReason::GrammarUnavailable,
        DegradationReason::IncompleteSyntax,
        DegradationReason::UnsafeSource,
        DegradationReason::UnsupportedEncoding,
        DegradationReason::LimitExceeded,
        DegradationReason::DynamicSource,
        DegradationReason::WorkerFailure,
    ];
    for variant in all {
        let json = serde_json::to_string(&variant).unwrap();
        let back: DegradationReason = serde_json::from_str(&json).unwrap();
        assert_eq!(back, variant);
    }
    assert_eq!(
        serde_json::to_string(&DegradationReason::LimitExceeded).unwrap(),
        "\"limit_exceeded\"",
    );
    assert_eq!(
        serde_json::to_string(&DegradationReason::WorkerFailure).unwrap(),
        "\"worker_failure\"",
    );
}

#[test]
fn degradation_reason_phrase_covers_every_variant() {
    let cases = [
        (
            DegradationReason::DynamicSource,
            "a command word, argument, or working directory is only known at run time",
        ),
        (
            DegradationReason::UnsafeSource,
            "a source file is missing, unreadable, or unsafe to read",
        ),
        (
            DegradationReason::LimitExceeded,
            "a size, depth, or time limit was reached",
        ),
        (
            DegradationReason::UnsupportedEncoding,
            "a source file uses an unsupported text encoding",
        ),
        (
            DegradationReason::IncompleteSyntax,
            "a script could not be fully parsed",
        ),
        (
            DegradationReason::GrammarUnavailable,
            "this build has no parser for the script's syntax",
        ),
        (
            DegradationReason::WorkerFailure,
            "the analysis worker failed",
        ),
    ];

    for (variant, expected) in cases {
        let phrase = variant.phrase();
        assert_eq!(phrase, expected, "unexpected phrase for {variant:?}");
        assert!(
            !phrase.to_ascii_lowercase().contains("language"),
            "phrase for {variant:?} must not mention language: {phrase:?}"
        );
    }
}

#[test]
fn operation_kind_round_trips_through_serde() {
    let all = [
        OperationKind::FilesystemDelete,
        OperationKind::FilesystemOverwrite,
        OperationKind::PermissionOrOwnershipChange,
        OperationKind::DeviceOrCriticalWrite,
        OperationKind::DatabaseDestructive,
        OperationKind::CodeExecution,
        OperationKind::CloudDestructive,
        OperationKind::ContainerDestructive,
        OperationKind::PackageDestructive,
    ];
    for variant in all {
        let json = serde_json::to_string(&variant).unwrap();
        let back: OperationKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, variant);
    }
    assert_eq!(
        serde_json::to_string(&OperationKind::CodeExecution).unwrap(),
        "\"code_execution\"",
    );
}

#[test]
fn operation_modifiers_default_all_false_and_round_trip() {
    let mods = OperationModifiers::default();
    assert!(!mods.recursive);
    assert!(!mods.forced);
    assert!(!mods.destructive_mode);

    let mods = OperationModifiers {
        recursive: true,
        forced: false,
        destructive_mode: true,
    };
    let json = serde_json::to_string(&mods).unwrap();
    let back: OperationModifiers = serde_json::from_str(&json).unwrap();
    assert_eq!(back, mods);
    assert!(json.contains("\"recursive\":true"));
    assert!(json.contains("\"destructive_mode\":true"));
    assert!(json.contains("\"forced\":false"));
}

#[test]
fn detected_operation_round_trips_and_preserves_certainty() {
    let op = DetectedOperation {
        kind: OperationKind::FilesystemDelete,
        modifiers: OperationModifiers {
            recursive: true,
            forced: true,
            destructive_mode: false,
        },
        certainty: OperandCertainty::Known,
    };
    let json = serde_json::to_string(&op).unwrap();
    let back: DetectedOperation = serde_json::from_str(&json).unwrap();
    assert_eq!(back, op);
    assert_eq!(back.certainty, OperandCertainty::Known);
    assert_eq!(back.kind, OperationKind::FilesystemDelete);
    assert!(back.modifiers.recursive && back.modifiers.forced);
}

#[test]
fn detected_operation_with_dynamic_certainty_is_not_known() {
    let op = DetectedOperation {
        kind: OperationKind::CodeExecution,
        modifiers: OperationModifiers::default(),
        certainty: OperandCertainty::Dynamic,
    };
    let back: DetectedOperation =
        serde_json::from_str(&serde_json::to_string(&op).unwrap()).unwrap();
    assert_eq!(back.certainty, OperandCertainty::Dynamic);
    assert_ne!(back.certainty, OperandCertainty::Known);
}

#[test]
fn source_origin_round_trips_through_serde() {
    for variant in [
        SourceOrigin::Inline,
        SourceOrigin::Heredoc,
        SourceOrigin::ScriptFile,
        SourceOrigin::Stdin,
        SourceOrigin::Pipe,
    ] {
        let json = serde_json::to_string(&variant).unwrap();
        let back: SourceOrigin = serde_json::from_str(&json).unwrap();
        assert_eq!(back, variant);
    }
    assert_eq!(
        serde_json::to_string(&SourceOrigin::ScriptFile).unwrap(),
        "\"script_file\"",
    );
}

#[test]
fn byte_span_round_trips_through_serde() {
    let span = ByteSpan {
        line: 3,
        column: 5,
        byte_start: 42,
        byte_end: 48,
    };
    let json = serde_json::to_string(&span).unwrap();
    let back: ByteSpan = serde_json::from_str(&json).unwrap();
    assert_eq!(back, span);
    assert!(json.contains("\"byte_start\":42"));
    assert!(json.contains("\"byte_end\":48"));
}

#[test]
fn analysis_provenance_round_trips_with_metadata_only() {
    let provenance = AnalysisProvenance {
        language: Some("python".to_string()),
        source_origin: SourceOrigin::Inline,
        rule_id: Some("PY-001".to_string()),
        operation: Some(DetectedOperation {
            kind: OperationKind::FilesystemDelete,
            modifiers: OperationModifiers {
                recursive: false,
                forced: false,
                destructive_mode: false,
            },
            certainty: OperandCertainty::Known,
        }),
        file_path: None,
        source_hash: Some("deadbeef".to_string()),
        span: Some(ByteSpan {
            line: 1,
            column: 1,
            byte_start: 0,
            byte_end: 10,
        }),
        certainty: OperandCertainty::Known,
        status: AnalysisStatus::Complete,
        degradation_reason: None,
    };
    let json = serde_json::to_string(&provenance).unwrap();
    let back: AnalysisProvenance = serde_json::from_str(&json).unwrap();
    assert_eq!(back, provenance);
    assert_eq!(back.language.as_deref(), Some("python"));
}

#[test]
fn analysis_provenance_serialized_form_carries_no_source_body_or_snippet() {
    let provenance = AnalysisProvenance {
        language: Some("python".to_string()),
        source_origin: SourceOrigin::ScriptFile,
        rule_id: Some("PY-002".to_string()),
        operation: None,
        file_path: Some("/tmp/x.py".to_string()),
        source_hash: Some("abc123".to_string()),
        span: None,
        certainty: OperandCertainty::Partial,
        status: AnalysisStatus::Degraded,
        degradation_reason: Some(DegradationReason::DynamicSource),
    };
    let json = serde_json::to_string(&provenance).unwrap();
    let obj: serde_json::Value = serde_json::from_str(&json).unwrap();
    let keys: Vec<&str> = obj
        .as_object()
        .expect("provenance serializes to a JSON object")
        .keys()
        .map(String::as_str)
        .collect();
    let forbidden = [
        "body",
        "snippet",
        "source",
        "source_body",
        "source_contents",
        "contents",
        "text",
        "ast",
        "syntax_tree",
        "value",
        "variable_value",
        "imported_source",
    ];
    for key in forbidden {
        assert!(
            !keys.contains(&key),
            "provenance leaked forbidden source-bearing key {key:?} in {keys:?}",
        );
    }
    assert!(keys.contains(&"file_path"));
    assert!(keys.contains(&"source_hash"));
}

#[test]
fn target_analysis_round_trips_and_status_orders_toward_degraded() {
    let complete = TargetAnalysis {
        status: AnalysisStatus::Complete,
        degradation_reasons: Vec::new(),
        provenance: None,
    };
    let degraded = TargetAnalysis {
        status: AnalysisStatus::Degraded,
        degradation_reasons: vec![DegradationReason::LimitExceeded],
        provenance: None,
    };
    assert_eq!(
        complete.status.max(degraded.status),
        AnalysisStatus::Degraded,
    );
    let json = serde_json::to_string(&degraded).unwrap();
    let back: TargetAnalysis = serde_json::from_str(&json).unwrap();
    assert_eq!(back, degraded);
    assert_eq!(
        back.degradation_reasons,
        vec![DegradationReason::LimitExceeded]
    );
}

#[test]
fn match_evidence_regex_round_trips_and_projects_to_mechanism() {
    let evidence = MatchEvidence::RegexPattern {
        source: DetectionSource::Builtin,
    };
    let json = serde_json::to_string(&evidence).unwrap();
    let back: MatchEvidence = serde_json::from_str(&json).unwrap();
    assert_eq!(back, evidence);
    assert_eq!(evidence.mechanism(), DetectionMechanism::RegexPattern);
    assert_eq!(evidence.source(), DetectionSource::Builtin);
    assert!(json.contains("\"kind\":\"regex_pattern\""));
    assert!(json.contains("\"source\":\"builtin\""));
}

#[test]
fn match_evidence_token_prefix_round_trips() {
    let evidence = MatchEvidence::TokenPrefixRule {
        source: DetectionSource::Custom,
    };
    let json = serde_json::to_string(&evidence).unwrap();
    let back: MatchEvidence = serde_json::from_str(&json).unwrap();
    assert_eq!(back, evidence);
    assert_eq!(evidence.mechanism(), DetectionMechanism::TokenPrefixRule);
    assert_eq!(evidence.source(), DetectionSource::Custom);
}

#[test]
fn match_evidence_language_carries_operation_and_provenance() {
    let operation = DetectedOperation {
        kind: OperationKind::CodeExecution,
        modifiers: OperationModifiers::default(),
        certainty: OperandCertainty::Dynamic,
    };
    let provenance = AnalysisProvenance {
        language: Some("javascript".to_string()),
        source_origin: SourceOrigin::Inline,
        rule_id: Some("JS-001".to_string()),
        operation: Some(operation.clone()),
        file_path: None,
        source_hash: Some("feedface".to_string()),
        span: None,
        certainty: OperandCertainty::Dynamic,
        status: AnalysisStatus::Degraded,
        degradation_reason: Some(DegradationReason::DynamicSource),
    };
    let evidence = MatchEvidence::LanguageRule {
        source: DetectionSource::Builtin,
        operation: operation.clone(),
        provenance: provenance.clone(),
    };
    let json = serde_json::to_string(&evidence).unwrap();
    let back: MatchEvidence = serde_json::from_str(&json).unwrap();
    assert_eq!(back, evidence);
    assert_eq!(evidence.mechanism(), DetectionMechanism::LanguageRule);
    assert_eq!(evidence.source(), DetectionSource::Builtin);
    assert!(json.contains("\"kind\":\"language_rule\""));
    assert!(json.contains("\"kind\":\"code_execution\""));
    assert!(json.contains("\"certainty\":\"dynamic\""));
}
