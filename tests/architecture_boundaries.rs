#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail loudly"
)]
use std::collections::BTreeSet;
use std::fs;

mod support;
use support::{assert_no_dep, repo_root};

fn read_file(relative: &str) -> String {
    fs::read_to_string(repo_root().join(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"))
}

#[test]
fn public_api_surface_is_stable() {
    let src = read_file("src/lib.rs");

    let expected: BTreeSet<&str> = [
        "analysis",
        "error",
        "explanation",
        "planning",
        "runtime",
        "runtime_gate",
        "toggle",
        "update",
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
        "public API surface changed — update CONVENTION.md §3 and this test.\n\
         added modules: {added:?}\nremoved modules: {removed:?}"
    );
}

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

#[test]
fn aegis_parser_must_not_depend_on_aegis_scanner() {
    assert_no_dep("aegis-parser", "aegis-scanner");
}

#[test]
fn aegis_parser_must_not_depend_on_aegis_policy() {
    assert_no_dep("aegis-parser", "aegis-policy");
}

#[test]
fn aegis_scanner_must_not_depend_on_aegis_policy() {
    assert_no_dep("aegis-scanner", "aegis-policy");
}

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

#[test]
fn aegis_explanation_must_not_depend_on_aegis_tui() {
    assert_no_dep("aegis-explanation", "aegis-tui");
}

#[test]
fn aegis_explanation_must_not_depend_on_aegis_audit() {
    assert_no_dep("aegis-explanation", "aegis-audit");
}

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

#[test]
fn aegis_tui_must_not_depend_on_aegis_audit() {
    assert_no_dep("aegis-tui", "aegis-audit");
}
