//! Pattern length limit (issue #217) as `config validate` reports it.

use super::{ConfigSourceMap, ValidationReport, validate_config};
use crate::allowlist::ConfigSourceLayer;
use crate::model::UserPattern;
use crate::{AegisConfig, AllowlistRule};
use aegis_types::{Category, RiskLevel};

fn user_pattern(id: &str, pattern: &str) -> UserPattern {
    UserPattern {
        id: id.to_string(),
        category: Category::Filesystem,
        risk: RiskLevel::Warn,
        pattern: pattern.to_string(),
        description: "test pattern".to_string(),
        safe_alt: None,
        justification: None,
    }
}

fn scoped_allow_rule(pattern: String) -> AllowlistRule {
    AllowlistRule {
        pattern,
        cwd: Some("/work".to_string()),
        user: None,
        expires_at: None,
        reason: "length probe".to_string(),
    }
}

fn first_error_message(report: &ValidationReport, code: &str) -> String {
    report
        .errors
        .iter()
        .find(|e| e.code == code)
        .unwrap_or_else(|| panic!("no {code} issue in {:?}", report.errors))
        .message
        .clone()
}

#[test]
fn validate_reports_custom_pattern_over_256_bytes() {
    let config = AegisConfig {
        custom_patterns: vec![user_pattern("USR-LONG", &"a".repeat(257))],
        custom_pattern_layers: vec![ConfigSourceLayer::Project],
        ..AegisConfig::defaults()
    };
    let report = validate_config(&config, &ConfigSourceMap::for_config(&config));

    assert!(!report.valid);
    let issue = report
        .errors
        .iter()
        .find(|e| e.code == "invalid_custom_pattern")
        .expect("invalid_custom_pattern issue");
    assert!(issue.message.contains("257 bytes"), "{}", issue.message);
    assert!(issue.message.contains("256"), "{}", issue.message);
    assert_eq!(issue.location, "project:custom_patterns[0]");
}

#[test]
fn validate_accepts_custom_pattern_of_256_bytes() {
    let config = AegisConfig {
        custom_patterns: vec![user_pattern("USR-EDGE", &"a".repeat(256))],
        ..AegisConfig::defaults()
    };
    let report = validate_config(&config, &ConfigSourceMap::for_config(&config));
    assert!(report.valid, "{:?}", report.errors);
}

#[test]
fn validate_measures_custom_pattern_length_in_bytes() {
    let config = AegisConfig {
        custom_patterns: vec![user_pattern("USR-UTF8", &"é".repeat(129))],
        ..AegisConfig::defaults()
    };
    let report = validate_config(&config, &ConfigSourceMap::for_config(&config));
    assert!(first_error_message(&report, "invalid_custom_pattern").contains("258 bytes"));
}

#[test]
fn validate_reports_allowlist_pattern_over_256_bytes() {
    let config = AegisConfig {
        allowlist: vec![scoped_allow_rule("a".repeat(257))],
        allowlist_layers: vec![ConfigSourceLayer::Project],
        ..AegisConfig::defaults()
    };
    let report = validate_config(&config, &ConfigSourceMap::for_config(&config));

    let message = first_error_message(&report, "invalid_allowlist_rule");
    assert!(message.contains("257 bytes"), "{message}");
    assert!(message.contains("256"), "{message}");
    let issue = report
        .errors
        .iter()
        .find(|e| e.code == "invalid_allowlist_rule")
        .unwrap();
    assert_eq!(issue.location, "project:allowlist[0]");
}

#[test]
fn validate_reports_blocklist_pattern_over_256_bytes() {
    let config = AegisConfig {
        blocklist: vec![crate::BlockRule {
            pattern: "a".repeat(257),
            cwd: Some("/work".to_string()),
            user: None,
            expires_at: None,
            reason: "length probe".to_string(),
        }],
        blocklist_layers: vec![ConfigSourceLayer::Global],
        ..AegisConfig::defaults()
    };
    let report = validate_config(&config, &ConfigSourceMap::for_config(&config));

    let message = first_error_message(&report, "invalid_allowlist_rule");
    assert!(message.contains("257 bytes"), "{message}");
    let issue = report
        .errors
        .iter()
        .find(|e| e.code == "invalid_allowlist_rule")
        .unwrap();
    assert_eq!(issue.location, "global:blocklist[0]");
}
