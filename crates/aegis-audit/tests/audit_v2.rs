use std::fs;

use aegis_audit::{AuditEntry, AuditLogger, AuditQuery, MatchedPattern};
use aegis_config::{AuditConfig, AuditIntegrityMode};
use aegis_types::Decision;
use aegis_types::RiskLevel;
use serde_json::Value;
use tempfile::TempDir;

fn v2_line() -> String {
    let operation = serde_json::json!({
        "kind": "filesystem_delete",
        "modifiers": {"recursive": false, "forced": false, "destructive_mode": false},
        "certainty": "known",
    });
    let provenance = serde_json::json!({
        "language": "python",
        "source_origin": "inline",
        "rule_id": "PY-FS-DEL",
        "operation": operation,
        "file_path": null,
        "source_hash": "abcdef0123456789",
        "span": {"line": 1, "column": 1, "byte_start": 0, "byte_end": 20},
        "certainty": "known",
        "status": "complete",
        "degradation_reason": null,
    });
    let value = serde_json::json!({
        "timestamp": "2023-11-14T22:13:20Z",
        "sequence": 2,
        "command": "python3 -c \"os.remove('/tmp/x')\"",
        "risk": "Danger",
        "matched_patterns": [{
            "id": "PY-FS-DEL",
            "risk": "Danger",
            "description": "filesystem delete",
            "safe_alt": null,
            "category": "Filesystem",
            "matched_text": "os.remove('/tmp/x')",
            "source": "builtin",
            "detection_id": "PY-FS-DEL",
            "evidence": {
                "kind": "language_rule",
                "source": "builtin",
                "operation": operation,
                "provenance": provenance,
            },
        }],
        "pattern_ids": ["PY-FS-DEL"],
        "decision": "Approved",
        "snapshots": [],
        "basis": {"kind": "decisive", "match_ids": ["PY-FS-DEL"]},
        "analysis": {"status": "complete", "degradation_reasons": []},
        "sandbox_status": "not_configured",
    });
    serde_json::to_string(&value).unwrap()
}

fn v1_line() -> &'static str {
    r#"{"timestamp":"2023-11-14T22:13:19Z","sequence":1,"command":"printf one","risk":"Safe","matched_patterns":[],"pattern_ids":[],"decision":"AutoApproved","snapshots":[],"sandbox_status":"not_configured"}"#
}

#[test]
fn v2_line_round_trips_typed_matches_basis_and_analysis() {
    let entry: AuditEntry = serde_json::from_str(&v2_line()).unwrap();
    let reserialized = serde_json::to_string(&entry).unwrap();
    let v: Value = serde_json::from_str(&reserialized).unwrap();

    assert_eq!(
        v["basis"]["kind"].as_str(),
        Some("decisive"),
        "basis kind must survive round-trip: {reserialized}",
    );
    assert_eq!(v["basis"]["match_ids"][0].as_str(), Some("PY-FS-DEL"));

    assert_eq!(v["analysis"]["status"].as_str(), Some("complete"));
    assert!(
        v["analysis"]["degradation_reasons"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    assert_eq!(
        v["matched_patterns"][0]["evidence"]["kind"].as_str(),
        Some("language_rule"),
        "typed Match evidence must survive round-trip: {reserialized}",
    );
    assert_eq!(
        v["matched_patterns"][0]["evidence"]["operation"]["kind"].as_str(),
        Some("filesystem_delete"),
    );
    assert_eq!(
        v["matched_patterns"][0]["evidence"]["provenance"]["source_origin"].as_str(),
        Some("inline"),
    );
    assert_eq!(
        v["matched_patterns"][0]["detection_id"].as_str(),
        Some("PY-FS-DEL"),
        "stable detection ID must survive round-trip: {reserialized}",
    );
}

#[test]
fn v1_line_deserializes_with_every_v2_field_absent() {
    let entry: AuditEntry = serde_json::from_str(v1_line()).unwrap();
    let reserialized = serde_json::to_string(&entry).unwrap();
    let v: Value = serde_json::from_str(&reserialized).unwrap();

    assert!(
        v.get("basis").is_none(),
        "v1 line must not carry basis: {reserialized}",
    );
    assert!(
        v.get("analysis").is_none(),
        "v1 line must not carry analysis: {reserialized}",
    );
    assert!(v["matched_patterns"].as_array().unwrap().is_empty());
}

#[test]
fn mixed_v1_v2_log_verifies_integrity_and_tampering_v2_basis_breaks_chain() {
    let home = TempDir::new().unwrap();
    let path = home.path().join("audit.jsonl");
    let logger = AuditLogger::new(&path).with_integrity_mode(AuditIntegrityMode::ChainSha256);

    let v1_entry = AuditEntry::new(
        "printf one",
        RiskLevel::Safe,
        Vec::<MatchedPattern>::new(),
        Decision::AutoApproved,
        Vec::new(),
        None,
        None,
    );
    logger.append(v1_entry).unwrap();

    let v2_entry: AuditEntry = serde_json::from_str(&v2_line()).unwrap();
    logger.append(v2_entry).unwrap();

    let report = logger.verify_integrity().unwrap();
    assert!(
        report.status == aegis_audit::AuditIntegrityStatus::Verified,
        "mixed v1/v2 log must verify: {:?}",
        report.message,
    );

    let on_disk = fs::read_to_string(&path).unwrap();
    assert!(
        on_disk.contains("\"basis\":{\"kind\":\"decisive\""),
        "v2 entry must have written basis to the log: {on_disk}",
    );
    let tampered = on_disk.replace(
        "\"basis\":{\"kind\":\"decisive\"",
        "\"basis\":{\"kind\":\"fallback\"",
    );
    fs::write(&path, tampered).unwrap();

    let report = logger.verify_integrity().unwrap();
    assert!(
        report.status == aegis_audit::AuditIntegrityStatus::Corrupt,
        "tampering v2 basis must break the chain: {:?}",
        report.message,
    );
}

#[test]
fn mixed_v1_v2_log_query_returns_both_entries() {
    let home = TempDir::new().unwrap();
    let path = home.path().join("audit.jsonl");
    let logger = AuditLogger::new(&path).with_integrity_mode(AuditIntegrityMode::ChainSha256);

    logger
        .append(AuditEntry::new(
            "printf one",
            RiskLevel::Safe,
            Vec::<MatchedPattern>::new(),
            Decision::AutoApproved,
            Vec::new(),
            None,
            None,
        ))
        .unwrap();
    let v2_entry: AuditEntry = serde_json::from_str(&v2_line()).unwrap();
    logger.append(v2_entry).unwrap();

    let entries = logger.query(AuditQuery::default()).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].as_base().command, "printf one");
    assert_eq!(
        entries[1].as_base().command,
        "python3 -c \"os.remove('/tmp/x')\""
    );
}

#[test]
fn mixed_v1_v2_log_survives_rotation_into_archive() {
    let home = TempDir::new().unwrap();
    let path = home.path().join("audit.jsonl");
    let config = AuditConfig {
        rotation_enabled: true,
        max_file_size_bytes: 1,
        retention_files: 3,
        compress_rotated: false,
        integrity_mode: AuditIntegrityMode::ChainSha256,
    };
    let logger = AuditLogger::from_audit_config(&config).with_path(&path);

    let v1_entry = || {
        AuditEntry::new(
            "printf one",
            RiskLevel::Safe,
            Vec::<MatchedPattern>::new(),
            Decision::AutoApproved,
            Vec::new(),
            None,
            None,
        )
    };
    let v2_entry = || serde_json::from_str::<AuditEntry>(&v2_line()).unwrap();
    for entry in [v1_entry(), v2_entry(), v1_entry(), v2_entry()] {
        logger.append(entry).unwrap();
    }

    let archive = home.path().join("audit.jsonl.1");
    assert!(
        archive.exists(),
        "rotation must archive a mixed v1/v2 segment"
    );

    let report = logger.verify_integrity().unwrap();
    assert_eq!(
        report.status,
        aegis_audit::AuditIntegrityStatus::Verified,
        "rotated mixed v1/v2 log must verify: {:?}",
        report.message,
    );
}

fn privacy_v2_line() -> String {
    let operation = serde_json::json!({
        "kind": "filesystem_delete",
        "modifiers": {"recursive": true, "forced": false, "destructive_mode": false},
        "certainty": "partial",
    });
    let provenance = serde_json::json!({
        "language": "python",
        "source_origin": "script_file",
        "rule_id": "PY-FS-DEL",
        "operation": operation,
        "file_path": "/workspace/cleanup.py",
        "source_hash": "0123456789abcdef",
        "span": {"line": 12, "column": 5, "byte_start": 88, "byte_end": 104},
        "certainty": "partial",
        "status": "degraded",
        "degradation_reason": "dynamic_source",
    });
    serde_json::to_string(&serde_json::json!({
        "timestamp": "2023-11-14T22:13:20Z",
        "sequence": 1,
        "command": "python3 ./cleanup.py",
        "risk": "Danger",
        "matched_patterns": [{
            "id": "PY-FS-DEL",
            "risk": "Danger",
            "description": "filesystem delete",
            "safe_alt": null,
            "category": "Filesystem",
            "matched_text": "os.remove",
            "source": "builtin",
            "detection_id": "PY-FS-DEL",
            "evidence": {
                "kind": "language_rule",
                "source": "builtin",
                "operation": operation,
                "provenance": provenance,
            },
        }],
        "pattern_ids": ["PY-FS-DEL"],
        "decision": "Approved",
        "snapshots": [],
        "basis": {"kind": "decisive", "match_ids": ["PY-FS-DEL"]},
        "analysis": {"status": "degraded", "degradation_reasons": ["dynamic_source"]},
        "sandbox_status": "not_configured",
    }))
    .unwrap()
}

fn collect_keys(v: &Value, out: &mut Vec<String>) {
    if let Some(obj) = v.as_object() {
        for (k, child) in obj {
            out.push(k.clone());
            collect_keys(child, out);
        }
    }
    if let Some(arr) = v.as_array() {
        for item in arr {
            collect_keys(item, out);
        }
    }
}

#[test]
fn v2_audit_entry_persists_only_allowed_provenance_fields() {
    let entry: AuditEntry = serde_json::from_str(&privacy_v2_line()).unwrap();
    let serialized = serde_json::to_string(&entry).unwrap();
    let v: Value = serde_json::from_str(&serialized).unwrap();

    let provenance = &v["matched_patterns"][0]["evidence"]["provenance"];
    let provenance_obj = provenance
        .as_object()
        .expect("provenance must serialize as an object");

    let allowed = [
        "language",
        "source_origin",
        "rule_id",
        "operation",
        "file_path",
        "source_hash",
        "span",
        "certainty",
        "status",
        "degradation_reason",
    ];
    let actual: Vec<&str> = provenance_obj.keys().map(String::as_str).collect();
    let mut expected = allowed.to_vec();
    expected.sort_unstable();
    let mut actual_sorted = actual.clone();
    actual_sorted.sort_unstable();
    assert_eq!(
        actual_sorted, expected,
        "provenance must carry exactly the allowed metadata fields: {serialized}",
    );

    assert_eq!(provenance_obj["language"].as_str(), Some("python"));
    assert_eq!(
        provenance_obj["source_hash"].as_str(),
        Some("0123456789abcdef")
    );
    assert_eq!(
        provenance_obj["file_path"].as_str(),
        Some("/workspace/cleanup.py")
    );
    assert_eq!(provenance_obj["span"]["byte_start"].as_u64(), Some(88));
}

#[test]
fn v2_audit_entry_serializes_no_source_body_snippet_ast_or_value_keys() {
    let entry: AuditEntry = serde_json::from_str(&privacy_v2_line()).unwrap();
    let serialized = serde_json::to_string(&entry).unwrap();
    let v: Value = serde_json::from_str(&serialized).unwrap();

    let mut keys = Vec::new();
    collect_keys(&v, &mut keys);

    let forbidden = [
        "source_body",
        "source_text",
        "source_content",
        "snippet",
        "snippets",
        "contents",
        "content",
        "body",
        "ast",
        "syntax_tree",
        "tree",
        "imported_source",
        "imported",
        "script",
        "script_contents",
        "variable_value",
        "value",
        "code",
        "payload",
    ];
    for key in &keys {
        assert!(
            !forbidden.contains(&key.as_str()),
            "audit JSON must not persist a forbidden source-content key {:?}: {serialized}",
            key,
        );
    }
}

#[test]
fn v2_entry_still_projects_v1_matched_patterns_and_pattern_ids() {
    let entry: AuditEntry = serde_json::from_str(&v2_line()).unwrap();
    let reserialized = serde_json::to_string(&entry).unwrap();
    let v: Value = serde_json::from_str(&reserialized).unwrap();

    assert_eq!(v["pattern_ids"][0].as_str(), Some("PY-FS-DEL"));

    let pat = &v["matched_patterns"][0];
    assert_eq!(pat["id"].as_str(), Some("PY-FS-DEL"));
    assert_eq!(pat["risk"].as_str(), Some("Danger"));
    assert!(pat["description"].as_str().is_some());
    assert!(pat.get("safe_alt").is_some(), "v1 safe_alt must remain");
    assert_eq!(pat["category"].as_str(), Some("Filesystem"));
    assert_eq!(pat["matched_text"].as_str(), Some("os.remove('/tmp/x')"));
    assert_eq!(pat["source"].as_str(), Some("builtin"));
    assert_eq!(pat["detection_id"].as_str(), Some("PY-FS-DEL"));
    assert_eq!(pat["evidence"]["kind"].as_str(), Some("language_rule"));
}

#[test]
fn v1_only_log_remains_queryable_through_v2_aware_codebase() {
    let home = TempDir::new().unwrap();
    let path = home.path().join("audit.jsonl");
    let logger = AuditLogger::new(&path).with_integrity_mode(AuditIntegrityMode::ChainSha256);

    logger
        .append(AuditEntry::new(
            "git stash clear",
            RiskLevel::Warn,
            vec![MatchedPattern {
                id: "GIT-007".to_string(),
                risk: RiskLevel::Warn,
                description: "destructive git form".to_string(),
                safe_alt: None,
                category: None,
                matched_text: Some("stash clear".to_string()),
                source: None,
                evidence: None,
                detection_id: None,
            }],
            Decision::Denied,
            Vec::new(),
            None,
            None,
        ))
        .unwrap();
    logger
        .append(AuditEntry::new(
            "printf one",
            RiskLevel::Safe,
            Vec::<MatchedPattern>::new(),
            Decision::AutoApproved,
            Vec::new(),
            None,
            None,
        ))
        .unwrap();

    let entries = logger.query(AuditQuery::default()).unwrap();
    assert_eq!(entries.len(), 2);

    let git = entries
        .iter()
        .find(|e| e.as_base().command == "git stash clear")
        .expect("v1 git entry must be queryable");
    assert_eq!(git.as_base().risk, RiskLevel::Warn);
    assert_eq!(git.as_base().decision, Decision::Denied);
    assert_eq!(git.as_base().pattern_ids, vec!["GIT-007".to_string()]);
    assert_eq!(git.as_base().matched_patterns.len(), 1);
    assert_eq!(git.as_base().matched_patterns[0].id, "GIT-007");
    assert!(git.as_base().basis.is_none());
    assert!(git.as_base().analysis.is_none());
    assert!(git.as_base().matched_patterns[0].evidence.is_none());
    assert!(git.as_base().matched_patterns[0].detection_id.is_none());
}

use std::borrow::Cow;
use std::sync::Arc;

use aegis_types::{
    AnalysisProvenance, AnalysisStatus, Category, DetectedOperation, DetectionSource,
    HighlightRange, MatchEvidence, MatchResult, OperandCertainty, OperationKind,
    OperationModifiers, Pattern, PatternSource, SourceOrigin,
};

fn language_match(pattern_id: &str, rule_id: Option<&str>) -> MatchResult {
    let provenance = AnalysisProvenance {
        language: Some("python".to_string()),
        source_origin: SourceOrigin::Inline,
        rule_id: rule_id.map(str::to_string),
        operation: None,
        file_path: None,
        source_hash: Some("deadbeef".to_string()),
        span: None,
        certainty: OperandCertainty::Known,
        status: AnalysisStatus::Complete,
        degradation_reason: None,
    };
    MatchResult {
        pattern: Arc::new(Pattern {
            id: Cow::Owned(pattern_id.to_string()),
            category: Category::Filesystem,
            risk: RiskLevel::Danger,
            pattern: Cow::Borrowed(""),
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: None,
            source: PatternSource::Builtin,
        }),
        matched_text: String::new(),
        highlight_range: None::<HighlightRange>,
        evidence: MatchEvidence::LanguageRule {
            source: DetectionSource::Builtin,
            operation: DetectedOperation {
                kind: OperationKind::FilesystemDelete,
                modifiers: OperationModifiers::default(),
                certainty: OperandCertainty::Known,
            },
            provenance,
        },
    }
}

#[test]
fn language_rule_detection_id_uses_provenance_rule_id_when_present() {
    let m = language_match("PY-FS-DEL", Some("PY-RULE-001"));
    let pat = MatchedPattern::from(&m);
    assert_eq!(
        pat.detection_id.as_deref(),
        Some("PY-RULE-001"),
        "LanguageRule detection_id must be the provenance rule id, not the pattern id",
    );
    assert_eq!(pat.id, "PY-FS-DEL");
}

#[test]
fn language_rule_detection_id_falls_back_to_pattern_id_when_rule_id_absent() {
    let m = language_match("PY-FS-DEL", None);
    let pat = MatchedPattern::from(&m);
    assert_eq!(
        pat.detection_id.as_deref(),
        Some("PY-FS-DEL"),
        "LanguageRule without a rule id falls back to the pattern id",
    );
}

#[test]
fn scanner_match_detection_id_uses_pattern_id() {
    let m = MatchResult {
        pattern: Arc::new(Pattern {
            id: Cow::Borrowed("FS-001"),
            category: Category::Filesystem,
            risk: RiskLevel::Danger,
            pattern: Cow::Borrowed(""),
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: None,
            source: PatternSource::Builtin,
        }),
        matched_text: String::new(),
        highlight_range: None::<HighlightRange>,
        evidence: MatchEvidence::RegexPattern {
            source: DetectionSource::Builtin,
        },
    };
    let pat = MatchedPattern::from(&m);
    assert_eq!(
        pat.detection_id.as_deref(),
        Some("FS-001"),
        "regex/token-prefix detection_id is the pattern id",
    );
}
