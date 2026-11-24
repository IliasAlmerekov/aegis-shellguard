//! Default-build contract tests for the opt-in `language-analysis` feature (ADR-024, ADR-054, #212).
//!
//! Pins that on the default build (without `--features language-analysis`):
//! - an existing `.aegis.toml` carrying `[language_analysis]` parses and keeps
//!   the project-config tightening rule;
//! - audit entries omit `analysis` for resolved script targets while marking
//!   `effect_opaque = true`;
//! - audit entries record `analysis.status = "degraded"` for `DynamicSource`
//!   targets and never emit `complete` or grammar `LANG-*` matches.

#![cfg(not(feature = "language-analysis"))]

mod support;

use std::fs;

use aegis_config::AegisConfig;
use tempfile::TempDir;

use support::{base_command, read_audit_entries, write_global_config};

#[test]
fn project_language_analysis_config_parses_and_tightening_applies_on_default_build() {
    let home = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();

    write_global_config(
        home.path(),
        "[language_analysis]\nmax_depth = 4\nmax_targets = 10\n",
    );
    fs::write(
        project.path().join(".aegis.toml"),
        "[language_analysis]\nmax_depth = 16\nmax_targets = 4\n",
    )
    .unwrap();

    let loaded = AegisConfig::load_for(project.path(), Some(home.path())).unwrap();
    assert_eq!(loaded.language_analysis.max_depth, 4);
    assert_eq!(loaded.language_analysis.max_targets, 4);
}

#[test]
fn audit_omits_analysis_for_resolved_script_target_and_marks_effect_opaque() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    write_global_config(home.path(), "mode = \"Audit\"\n");
    fs::write(workspace.path().join("script.py"), "print('hi')\n").unwrap();

    let output = base_command(home.path())
        .current_dir(workspace.path())
        .args(["-c", "python3 script.py"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "audit mode should allow execution; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let entries = read_audit_entries(home.path());
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert!(
        entry.get("analysis").is_none(),
        "default build must omit `analysis` for resolved targets: {entry}"
    );
    assert_eq!(entry["effect_opaque"], true, "{entry}");
    let patterns = entry["matched_patterns"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    assert!(
        !patterns
            .iter()
            .any(|p| p.as_str().is_some_and(|id| id.starts_with("LANG-"))),
        "default build must never emit grammar LANG-* matches: {entry}"
    );
}

#[test]
fn audit_records_degraded_analysis_for_dynamic_source_on_default_build() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();

    let output = base_command(home.path())
        .current_dir(workspace.path())
        .env("AEGIS_FORCE_NO_TTY", "1")
        .args(["-c", "$X -rf /"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(2),
        "dynamic command word must be denied without TTY; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let entries = read_audit_entries(home.path());
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry["decision"], "Denied", "{entry}");
    assert_eq!(entry["analysis"]["status"], "degraded", "{entry}");
    let reasons = entry["analysis"]["degradation_reasons"]
        .as_array()
        .expect("degraded analysis must list degradation_reasons");
    assert!(
        reasons.iter().any(|r| r == "dynamic_source"),
        "expected dynamic_source in degradation_reasons: {entry}"
    );
    assert_ne!(entry["analysis"]["status"], "complete", "{entry}");
    let patterns = entry["matched_patterns"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    assert!(
        !patterns
            .iter()
            .any(|p| p.as_str().is_some_and(|id| id.starts_with("LANG-"))),
        "default build must never emit grammar LANG-* matches: {entry}"
    );
}
