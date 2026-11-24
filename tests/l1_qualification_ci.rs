use std::fs;
use std::path::Path;

fn repo_file(path: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("{} must be readable: {err}", path.display()))
}

#[test]
fn language_analysis_feature_stays_opt_in_and_out_of_default() {
    let manifest: toml::Table = repo_file("Cargo.toml")
        .parse()
        .expect("root Cargo.toml must parse");
    let features = manifest
        .get("features")
        .and_then(toml::Value::as_table)
        .expect("root Cargo.toml must define [features]");
    assert!(
        features.contains_key("language-analysis"),
        "root Cargo.toml must declare the `language-analysis` feature (ADR-024, ADR-054)"
    );
    let default_features: Vec<&str> = features
        .get("default")
        .and_then(toml::Value::as_array)
        .expect("root Cargo.toml must define [features].default")
        .iter()
        .filter_map(toml::Value::as_str)
        .collect();
    assert!(
        !default_features.contains(&"language-analysis"),
        "`language-analysis` must stay excluded from default features: {default_features:?}"
    );

    let lang_manifest: toml::Table = repo_file("crates/aegis-language/Cargo.toml")
        .parse()
        .expect("crates/aegis-language/Cargo.toml must parse");
    let lang_deps = lang_manifest
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .expect("aegis-language must define [dependencies]");
    for (dep, spec) in lang_deps {
        if dep.starts_with("tree-sitter") {
            let optional = spec
                .as_table()
                .and_then(|t| t.get("optional"))
                .and_then(toml::Value::as_bool)
                .unwrap_or(false);
            assert!(
                optional,
                "aegis-language dependency `{dep}` must be optional so default builds link no Tree-sitter crate"
            );
        }
    }
    let lang_default: Vec<&str> = lang_manifest
        .get("features")
        .and_then(toml::Value::as_table)
        .and_then(|f| f.get("default"))
        .and_then(toml::Value::as_array)
        .map(|arr| arr.iter().filter_map(toml::Value::as_str).collect())
        .unwrap_or_default();
    assert!(
        !lang_default.contains(&"grammars"),
        "aegis-language must not enable `grammars` in default features: {lang_default:?}"
    );
}

#[test]
fn every_release_target_builds_the_shipping_binary_in_release_mode() {
    // The build commands live in the `build-target` composite action, which
    // ci.yml and release.yml both call, so CI compiles the same binary a
    // release ships rather than a second copy of the command line.
    let action = repo_file(".github/actions/build-target/action.yml");

    assert!(
        action.contains("cross build --release --target ${{ inputs.target }}"),
        "musl release targets must build the shipping binary in release mode"
    );
    assert!(
        action.contains("cargo build --release --target ${{ inputs.target }}"),
        "macOS release targets must build the shipping binary in release mode"
    );

    assert!(
        !action.contains("--features language-analysis"),
        "default release targets must stay opt-in and not enable `--features language-analysis` (ADR-024, ADR-054)"
    );

    let workflow = repo_file(".github/workflows/ci.yml");
    assert!(
        workflow.contains("uses: ./.github/actions/build-target"),
        "release CI must compile every target through the build-target action"
    );
    assert!(
        workflow.contains("cargo test --workspace --features language-analysis")
            && workflow
                .contains("cargo clippy --workspace --all-targets --features language-analysis")
            && workflow.contains("cargo deny --features language-analysis check"),
        "CI must still exercise the opt-in `language-analysis` feature in a dedicated job (ADR-024 §7)"
    );
    assert!(
        workflow.contains("include: ${{ fromJSON(needs.gate.outputs.build_targets) }}"),
        "the CI cross matrix must expand .github/build-targets.json rather than \
         listing targets a second time"
    );

    let targets = repo_file(".github/build-targets.json");
    for target in [
        "x86_64-unknown-linux-musl",
        "aarch64-unknown-linux-musl",
        "x86_64-apple-darwin",
        "aarch64-apple-darwin",
    ] {
        assert!(
            targets.contains(target),
            "release CI must retain ADR-022 target `{target}`"
        );
    }
}
