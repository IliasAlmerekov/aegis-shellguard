#![expect(clippy::expect_used, reason = "test helpers fail loudly")]

use std::fs;
use std::process::Command;

const DENIED_LINTS: [&str; 5] = [
    "unwrap_used",
    "expect_used",
    "panic",
    "unreachable",
    "allow_attributes",
];

fn without_whitespace(text: &str) -> String {
    text.split_whitespace().collect()
}

fn inherits_workspace_lints(manifest: &str) -> bool {
    let code: String = manifest
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    without_whitespace(&code).contains("[lints]workspace=true")
}

fn denies_lint(manifest: &str, lint: &str) -> bool {
    manifest
        .lines()
        .any(|line| without_whitespace(line) == format!("{lint}=\"deny\""))
}

fn cargo_metadata() -> serde_json::Value {
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--offline",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo metadata should run");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("cargo metadata should print JSON")
}

fn manifest_paths(metadata: &serde_json::Value) -> Vec<String> {
    let mut paths: Vec<String> = metadata["packages"]
        .as_array()
        .expect("packages array")
        .iter()
        .map(|package| {
            package["manifest_path"]
                .as_str()
                .expect("manifest_path string")
                .to_owned()
        })
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

#[test]
fn every_workspace_package_inherits_the_workspace_lints() {
    let paths = manifest_paths(&cargo_metadata());
    assert!(!paths.is_empty(), "cargo metadata listed no packages");

    let missing: Vec<&str> = paths
        .iter()
        .filter(|path| {
            !inherits_workspace_lints(
                &fs::read_to_string(path).expect("package manifest should be readable"),
            )
        })
        .map(String::as_str)
        .collect();

    assert!(
        missing.is_empty(),
        "manifests missing `[lints] workspace = true`:\n{}",
        missing.join("\n")
    );
}

#[test]
fn the_workspace_denies_unwrap_expect_panic_unreachable_and_allow_attributes() {
    let manifest = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .expect("root manifest should be readable");

    let missing: Vec<&str> = DENIED_LINTS
        .into_iter()
        .filter(|lint| !denies_lint(&manifest, lint))
        .collect();

    assert!(
        missing.is_empty(),
        "root Cargo.toml does not deny: {missing:?}"
    );
}

#[test]
fn a_commented_out_lints_table_does_not_count() {
    assert!(inherits_workspace_lints("[lints]\nworkspace = true\n"));
    assert!(!inherits_workspace_lints("# [lints]\n# workspace = true\n"));
    assert!(!inherits_workspace_lints("[package]\nname = \"x\"\n"));
}
