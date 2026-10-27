//! `snapshot_attempts` in the audit schema and the integrity chain
//! (ADR-031 §8, §10, §11; #234).

use std::fs;
use std::path::{Path, PathBuf};

use aegis_audit::{AuditEntry, AuditIntegrityStatus, AuditLogger, MatchedPattern};
use aegis_config::AuditIntegrityMode;
use aegis_types::{
    Decision, RecoveryDegradation, RiskLevel, SnapshotAttempt, SnapshotAttemptReadiness,
    StructuralCheck, ValidationLevel,
};
use serde_json::{Value, json};
use tempfile::TempDir;

const LEGACY_LINE: &str = r#"{"timestamp":"2023-11-14T22:13:19Z","sequence":1,"command":"printf one","risk":"Safe","matched_patterns":[],"pattern_ids":[],"decision":"AutoApproved","snapshots":[],"sandbox_status":"NotConfigured"}"#;

/// Exact line the base code wrote for `LEGACY_LINE` before `snapshot_attempts` existed.
const LEGACY_WRITTEN: &str = r#"{"timestamp":"2023-11-14T22:13:19Z","sequence":1,"command":"printf one","risk":"Safe","matched_patterns":[],"pattern_ids":[],"decision":"AutoApproved","snapshots":[],"allowlist_matched":false,"allowlist_effective":false,"chain_alg":"sha256","entry_hash":"bc322500ad8eb9775e4bbfe9b7a53296bdbb2afad3f9c66ca451a657ebb2d34c","sandbox_status":"not_configured"}"#;

fn chained_logger(dir: &TempDir) -> (AuditLogger, PathBuf) {
    let path = dir.path().join("audit.jsonl");
    let logger = AuditLogger::new(&path).with_integrity_mode(AuditIntegrityMode::ChainSha256);
    (logger, path)
}

fn attempts() -> Vec<SnapshotAttempt> {
    vec![
        SnapshotAttempt {
            plugin: "git".to_string(),
            readiness: SnapshotAttemptReadiness::Ready,
            validation_level: ValidationLevel::Structural,
            structural_check: StructuralCheck::Passed,
        },
        SnapshotAttempt {
            plugin: "docker".to_string(),
            readiness: SnapshotAttemptReadiness::Unavailable,
            validation_level: ValidationLevel::PresenceOnly,
            structural_check: StructuralCheck::NotRun,
        },
    ]
}

fn entry_with_attempts() -> AuditEntry {
    AuditEntry::new(
        "printf one",
        RiskLevel::Safe,
        Vec::<MatchedPattern>::new(),
        Decision::AutoApproved,
        Vec::new(),
        None,
        None,
    )
    .with_snapshot_attempts(attempts())
}

/// Write one chained entry carrying attempts, return the logger and its file.
fn written_with_attempts(dir: &TempDir) -> (AuditLogger, PathBuf) {
    let (logger, path) = chained_logger(dir);
    logger.append(entry_with_attempts()).unwrap();
    (logger, path)
}

/// Rewrite the single on-disk line after `edit` changed its `snapshot_attempts`.
fn tamper_attempts(path: &Path, edit: impl FnOnce(&mut Vec<Value>)) {
    let mut line: Value = serde_json::from_str(fs::read_to_string(path).unwrap().trim()).unwrap();
    edit(line["snapshot_attempts"].as_array_mut().unwrap());
    fs::write(path, format!("{line}\n")).unwrap();
}

fn assert_corrupt(logger: &AuditLogger) {
    let report = logger.verify_integrity().unwrap();
    assert_eq!(
        report.status,
        AuditIntegrityStatus::Corrupt,
        "{:?}",
        report.message
    );
}

#[test]
fn legacy_line_written_form_is_byte_identical_and_verifies() {
    let dir = TempDir::new().unwrap();
    let (logger, path) = chained_logger(&dir);
    logger
        .append(serde_json::from_str::<AuditEntry>(LEGACY_LINE).unwrap())
        .unwrap();

    assert_eq!(
        fs::read_to_string(&path).unwrap().trim_end(),
        LEGACY_WRITTEN
    );
    assert_eq!(
        logger.verify_integrity().unwrap().status,
        AuditIntegrityStatus::Verified
    );
}

#[test]
fn attempt_line_appended_to_a_legacy_chain_links_and_verifies() {
    let dir = TempDir::new().unwrap();
    let (logger, path) = chained_logger(&dir);
    fs::write(&path, format!("{LEGACY_WRITTEN}\n")).unwrap();

    logger.append(entry_with_attempts()).unwrap();

    let contents = fs::read_to_string(&path).unwrap();
    let lines: Vec<Value> = contents
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines[1]["prev_hash"], lines[0]["entry_hash"]);
    assert_eq!(
        logger.verify_integrity().unwrap().status,
        AuditIntegrityStatus::Verified
    );
}

#[test]
fn line_with_two_attempts_verifies() {
    let dir = TempDir::new().unwrap();
    let (logger, _) = written_with_attempts(&dir);
    assert_eq!(
        logger.verify_integrity().unwrap().status,
        AuditIntegrityStatus::Verified
    );
}

#[test]
fn changing_one_attempt_readiness_breaks_verification() {
    let dir = TempDir::new().unwrap();
    let (logger, path) = written_with_attempts(&dir);
    tamper_attempts(&path, |a| a[1]["readiness"] = json!("ready"));
    assert_corrupt(&logger);
}

#[test]
fn reordering_attempts_breaks_verification() {
    let dir = TempDir::new().unwrap();
    let (logger, path) = written_with_attempts(&dir);
    tamper_attempts(&path, |a| a.swap(0, 1));
    assert_corrupt(&logger);
}

#[test]
fn deleting_an_attempt_breaks_verification() {
    let dir = TempDir::new().unwrap();
    let (logger, path) = written_with_attempts(&dir);
    tamper_attempts(&path, |a| {
        a.pop();
    });
    assert_corrupt(&logger);
}

#[test]
fn attempts_survive_write_then_read() {
    let dir = TempDir::new().unwrap();
    let (_, path) = written_with_attempts(&dir);
    let read: AuditEntry = serde_json::from_str(fs::read_to_string(&path).unwrap().trim()).unwrap();
    assert_eq!(read.as_base().snapshot_attempts, Some(attempts()));
}

#[test]
fn artifact_degradations_use_snake_case() {
    for (variant, text) in [
        (
            RecoveryDegradation::SnapshotArtifactUnavailable,
            "snapshot_artifact_unavailable",
        ),
        (
            RecoveryDegradation::SnapshotArtifactInvalid,
            "snapshot_artifact_invalid",
        ),
    ] {
        assert_eq!(serde_json::to_value(variant).unwrap(), json!(text));
        assert_eq!(
            serde_json::from_value::<RecoveryDegradation>(json!(text)).unwrap(),
            variant
        );
    }
}

#[test]
fn not_run_structural_check_uses_snake_case() {
    assert_eq!(
        serde_json::to_value(StructuralCheck::NotRun).unwrap(),
        json!("not_run")
    );
    assert_eq!(
        serde_json::from_value::<StructuralCheck>(json!("not_run")).unwrap(),
        StructuralCheck::NotRun
    );
}
