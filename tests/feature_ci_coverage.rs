use std::fs;
use std::path::Path;

fn repo_file(path: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("{} must be readable: {err}", path.display()))
}

fn root_features() -> Vec<String> {
    let manifest: toml::Table = repo_file("Cargo.toml").parse().expect("Cargo.toml parses");
    manifest
        .get("features")
        .and_then(toml::Value::as_table)
        .map(|features| features.keys().cloned().collect())
        .unwrap_or_default()
}

// PRD 5.2 / ADR-028: the Starlark policy DSL is gone, so no feature may bring it back.
#[test]
fn root_manifest_declares_no_starlark_policy_feature() {
    assert!(
        !root_features().iter().any(|name| name == "starlark-policy"),
        "the starlark-policy feature was removed by ADR-028"
    );
}

// Ratchet for #212: a new feature turns this red until CI tests, lints and
// audits it, so the CI job cannot be shaped around a feature that already exists.
#[test]
fn every_non_default_feature_is_tested_linted_and_denied_in_ci() {
    let ci = repo_file(".github/workflows/ci.yml");

    for name in root_features().iter().filter(|name| *name != "default") {
        for command in [
            format!("cargo test --workspace --features {name}"),
            format!("cargo clippy --workspace --all-targets --features {name}"),
            format!("cargo deny check --features {name}"),
        ] {
            assert!(
                ci.contains(&command),
                "ci.yml must run `{command}` for feature `{name}`"
            );
        }
    }
}
