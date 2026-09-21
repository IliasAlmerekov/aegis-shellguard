//! Every library and binary crate root must deny `unwrap` and `expect` outside
//! tests. A new crate or `[[bin]]` without the attribute fails here.

use std::fs;
use std::path::Path;
use std::process::Command;

const REQUIRED_ATTRIBUTE: &str =
    "#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]";

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

fn lib_and_bin_roots(metadata: &serde_json::Value) -> Vec<String> {
    let mut roots = Vec::new();
    for package in metadata["packages"].as_array().expect("packages array") {
        for target in package["targets"].as_array().expect("targets array") {
            let is_lib_or_bin = target["kind"]
                .as_array()
                .expect("kind array")
                .iter()
                .any(|kind| matches!(kind.as_str(), Some("lib" | "bin")));
            if is_lib_or_bin {
                roots.push(
                    target["src_path"]
                        .as_str()
                        .expect("src_path string")
                        .to_owned(),
                );
            }
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

#[test]
fn every_lib_and_bin_root_denies_unwrap_and_expect() {
    let roots = lib_and_bin_roots(&cargo_metadata());
    assert!(
        !roots.is_empty(),
        "cargo metadata listed no lib or bin targets"
    );

    let missing: Vec<&str> = roots
        .iter()
        .filter(|root| {
            !fs::read_to_string(Path::new(root))
                .expect("crate root should be readable")
                .lines()
                .any(|line| line.trim() == REQUIRED_ATTRIBUTE)
        })
        .map(String::as_str)
        .collect();

    assert!(
        missing.is_empty(),
        "crate roots missing `{REQUIRED_ATTRIBUTE}`:\n{}",
        missing.join("\n")
    );
}
