//! Architecture boundary tests.
//!
//! These tests enforce the workspace crate dependency DAG (`ARCHITECTURE.md`
//! §4) and the public API surface (§8). A PR that breaks one of these fails CI.
//!
//! Crate-to-crate edges are checked against each crate's `Cargo.toml`: a crate
//! cannot `use` another crate that is missing from its manifest. Source-grep
//! rules (tracing fields, engine purity, interceptor, UI, transport routing)
//! live in `tests/architecture_source_rules.rs`.

use std::collections::BTreeSet;
use std::fs;

mod common;
use common::{assert_no_dep, repo_root};

fn read_file(relative: &str) -> String {
    fs::read_to_string(repo_root().join(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"))
}

// ── §8 Public API surface ─────────────────────────────────────────────────────

/// §8: `src/lib.rs` exports a fixed set of modules. Adding or removing a
/// top-level module is a public-API change and requires updating
/// ARCHITECTURE.md §8 and this test together.
#[test]
fn public_api_surface_is_stable() {
    let src = read_file("src/lib.rs");

    let expected: BTreeSet<&str> = [
        "analysis",
        "audit",
        "config",
        "decision",
        "error",
        "explanation",
        "interceptor",
        "planning",
        "runtime",
        "runtime_gate",
        "snapshot",
        "toggle",
        "ui",
        "watch",
    ]
    .into_iter()
    .collect();

    let found: BTreeSet<String> = src
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix("pub mod ")?;
            let name = rest.trim_end_matches(';').trim_end_matches('{').trim();
            Some(name.to_string())
        })
        .collect();

    let found_refs: BTreeSet<&str> = found.iter().map(String::as_str).collect();

    let added: Vec<&&str> = found_refs.difference(&expected).collect();
    let removed: Vec<&&str> = expected.difference(&found_refs).collect();

    assert!(
        added.is_empty() && removed.is_empty(),
        "public API surface changed — update ARCHITECTURE.md §8 and this test.\n\
         added modules: {added:?}\nremoved modules: {removed:?}"
    );
}

// ── Workspace crate dependency DAG ───────────────────────────────────────────
//
// `assert_no_dep` / `crate_deps_section` / `repo_root` live in `tests/common`
// and are shared with `tests/aegis_language_boundary.rs`.

#[test]
fn aegis_parser_must_not_depend_on_aegis_audit() {
    assert_no_dep("aegis-parser", "aegis-audit");
}

#[test]
fn aegis_parser_must_not_depend_on_aegis_config() {
    assert_no_dep("aegis-parser", "aegis-config");
}

#[test]
fn aegis_parser_must_not_depend_on_aegis_explanation() {
    assert_no_dep("aegis-parser", "aegis-explanation");
}

#[test]
fn aegis_parser_must_not_depend_on_aegis_tui() {
    assert_no_dep("aegis-parser", "aegis-tui");
}

#[test]
fn aegis_parser_must_not_depend_on_aegis_snapshot() {
    assert_no_dep("aegis-parser", "aegis-snapshot");
}

#[test]
fn aegis_scanner_must_not_depend_on_aegis_audit() {
    assert_no_dep("aegis-scanner", "aegis-audit");
}

#[test]
fn aegis_scanner_must_not_depend_on_aegis_config() {
    assert_no_dep("aegis-scanner", "aegis-config");
}

#[test]
fn aegis_scanner_must_not_depend_on_aegis_explanation() {
    assert_no_dep("aegis-scanner", "aegis-explanation");
}

#[test]
fn aegis_scanner_must_not_depend_on_aegis_tui() {
    assert_no_dep("aegis-scanner", "aegis-tui");
}

#[test]
fn aegis_scanner_must_not_depend_on_aegis_snapshot() {
    assert_no_dep("aegis-scanner", "aegis-snapshot");
}

#[test]
fn aegis_types_must_not_depend_on_aegis_audit() {
    assert_no_dep("aegis-types", "aegis-audit");
}

#[test]
fn aegis_types_must_not_depend_on_aegis_config() {
    assert_no_dep("aegis-types", "aegis-config");
}

#[test]
fn aegis_types_must_not_depend_on_aegis_explanation() {
    assert_no_dep("aegis-types", "aegis-explanation");
}

#[test]
fn aegis_types_must_not_depend_on_aegis_tui() {
    assert_no_dep("aegis-types", "aegis-tui");
}

#[test]
fn aegis_types_must_not_depend_on_aegis_snapshot() {
    assert_no_dep("aegis-types", "aegis-snapshot");
}

// ── Missing edges for parser (scanner and policy are downstream) ──────────────

#[test]
fn aegis_parser_must_not_depend_on_aegis_scanner() {
    assert_no_dep("aegis-parser", "aegis-scanner");
}

#[test]
fn aegis_parser_must_not_depend_on_aegis_policy() {
    assert_no_dep("aegis-parser", "aegis-policy");
}

// ── Missing edges for scanner (policy is downstream) ─────────────────────────

#[test]
fn aegis_scanner_must_not_depend_on_aegis_policy() {
    assert_no_dep("aegis-scanner", "aegis-policy");
}

// ── Missing edges for types (all other crates are downstream) ────────────────

#[test]
fn aegis_types_must_not_depend_on_aegis_parser() {
    assert_no_dep("aegis-types", "aegis-parser");
}

#[test]
fn aegis_types_must_not_depend_on_aegis_scanner() {
    assert_no_dep("aegis-types", "aegis-scanner");
}

#[test]
fn aegis_types_must_not_depend_on_aegis_policy() {
    assert_no_dep("aegis-types", "aegis-policy");
}

// ── DAG boundaries for policy (config/explanation/tui/audit/snapshot are downstream) ──

#[test]
fn aegis_policy_must_not_depend_on_aegis_config() {
    assert_no_dep("aegis-policy", "aegis-config");
}

#[test]
fn aegis_policy_must_not_depend_on_aegis_explanation() {
    assert_no_dep("aegis-policy", "aegis-explanation");
}

#[test]
fn aegis_policy_must_not_depend_on_aegis_tui() {
    assert_no_dep("aegis-policy", "aegis-tui");
}

#[test]
fn aegis_policy_must_not_depend_on_aegis_audit() {
    assert_no_dep("aegis-policy", "aegis-audit");
}

#[test]
fn aegis_policy_must_not_depend_on_aegis_snapshot() {
    assert_no_dep("aegis-policy", "aegis-snapshot");
}

// ── DAG boundaries for config (explanation/tui/audit are downstream) ─────────

#[test]
fn aegis_config_must_not_depend_on_aegis_explanation() {
    assert_no_dep("aegis-config", "aegis-explanation");
}

#[test]
fn aegis_config_must_not_depend_on_aegis_tui() {
    assert_no_dep("aegis-config", "aegis-tui");
}

#[test]
fn aegis_config_must_not_depend_on_aegis_audit() {
    assert_no_dep("aegis-config", "aegis-audit");
}

#[test]
fn aegis_config_must_not_depend_on_aegis_snapshot() {
    assert_no_dep("aegis-config", "aegis-snapshot");
}

#[test]
fn aegis_config_must_not_depend_on_aegis_policy() {
    assert_no_dep("aegis-config", "aegis-policy");
}

// ── DAG boundaries for explanation (tui/audit are downstream) ────────────────

#[test]
fn aegis_explanation_must_not_depend_on_aegis_tui() {
    assert_no_dep("aegis-explanation", "aegis-tui");
}

#[test]
fn aegis_explanation_must_not_depend_on_aegis_audit() {
    assert_no_dep("aegis-explanation", "aegis-audit");
}

// ── DAG boundaries for snapshot (explanation/tui/audit/policy are downstream) ─

#[test]
fn aegis_snapshot_must_not_depend_on_aegis_explanation() {
    assert_no_dep("aegis-snapshot", "aegis-explanation");
}

#[test]
fn aegis_snapshot_must_not_depend_on_aegis_tui() {
    assert_no_dep("aegis-snapshot", "aegis-tui");
}

#[test]
fn aegis_snapshot_must_not_depend_on_aegis_audit() {
    assert_no_dep("aegis-snapshot", "aegis-audit");
}

#[test]
fn aegis_snapshot_must_not_depend_on_aegis_policy() {
    assert_no_dep("aegis-snapshot", "aegis-policy");
}

// ── DAG boundaries for tui (audit is downstream) ─────────────────────────────

#[test]
fn aegis_tui_must_not_depend_on_aegis_audit() {
    assert_no_dep("aegis-tui", "aegis-audit");
}
