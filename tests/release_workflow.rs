//! Regression tests for static-musl release targets.
//!
//! These tests encode the release-workflow target matrix contract: both Linux
//! musl targets build through `cross`, and static-binary verification runs
//! before checksum generation. The asset-name test preserves the installer
//! asset contract.

use std::path::Path;

fn release_workflow() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release.yml");
    std::fs::read_to_string(&path).expect("release workflow should be readable")
}

/// Extracts the single matrix entry for `target` from `.github/build-targets.json`.
/// The entry spans from its `"target": "<triple>"` marker up to the closing
/// brace of that object, so callers can assert on per-target fields like
/// `use_cross` without a JSON dependency.
///
/// Panics if the target is absent. That is a test-fixture failure, not a
/// runtime failure, so `panic!`/`expect` is acceptable here.
fn matrix_entry(targets: &str, target: &str) -> String {
    let marker = format!("\"target\": \"{target}\"");
    let entry = targets
        .split_once(marker.as_str())
        // The entry ends at the closing brace of its JSON object, so `entry`
        // is exactly this target's remaining fields.
        .and_then(|(_, rest)| rest.split_once('}'))
        .map(|(entry, _)| entry.to_owned())
        .unwrap_or_else(|| panic!("release workflow matrix should define target {target}"));

    format!("{marker}{entry}")
}

/// The release target matrix. `release.yml` (`build`) and `ci.yml`
/// (`cross-matrix`) both expand this file into `strategy.matrix.include`, so
/// it is where the target list, the runner labels, and the asset names live.
fn build_targets() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/build-targets.json");
    std::fs::read_to_string(&path).expect("build-targets.json should be readable")
}

/// The asset paths the GitHub Release publishes.
///
/// The workflow derives them from `build-targets.json` with jq instead of
/// listing them a second time, so this derives them the same way. Every
/// resulting line is a glob pattern that `softprops/action-gh-release` matches
/// with `fail_on_unmatched_files: true`, so a line that is not a bare path
/// aborts the release.
fn release_files_patterns() -> Vec<String> {
    let mut patterns: Vec<String> = build_targets()
        .split("\"asset_name\": \"")
        .skip(1)
        .filter_map(|fragment| fragment.split('"').next().map(str::to_owned))
        .flat_map(|asset| {
            [
                format!("artifacts/{asset}"),
                format!("artifacts/{asset}.sha256"),
            ]
        })
        .collect();
    patterns.push("THIRD_PARTY_NOTICES.md".to_owned());
    patterns
}

#[test]
fn release_workflow_files_block_should_contain_only_bare_asset_paths() {
    let wf = release_workflow();
    let patterns = release_files_patterns();

    assert!(
        !patterns.is_empty(),
        "GitHub Release `files:` block must list at least one asset"
    );
    assert!(
        wf.contains("files: ${{ steps.assets.outputs.files }}"),
        "the GitHub Release asset list must come from the step that derives it from \
         build-targets.json"
    );
    assert!(
        wf.contains(
            r#"jq -r '.targets[] | "artifacts/\(.asset_name)", "artifacts/\(.asset_name).sha256"'"#
        ),
        "the asset list step must emit a binary and a .sha256 sidecar for every target"
    );
    for pattern in &patterns {
        assert!(
            !pattern.starts_with('#'),
            "`{pattern}` reads as a comment rather than a path, so the action \
             treats it as an unmatched glob and fails the release"
        );
        assert!(
            !pattern.contains(','),
            "`{pattern}` contains a comma, which the action splits into extra patterns"
        );
    }
}

#[test]
fn release_workflow_should_build_linux_musl_targets() {
    let targets = build_targets();
    assert!(
        targets.contains("x86_64-unknown-linux-musl"),
        "release workflow must build x86_64-unknown-linux-musl"
    );
    assert!(
        targets.contains("aarch64-unknown-linux-musl"),
        "release workflow must build aarch64-unknown-linux-musl"
    );
    assert!(
        release_workflow().contains("include: ${{ fromJSON(needs.config.outputs.build_targets) }}"),
        "the release build matrix must expand .github/build-targets.json"
    );
}

#[test]
fn release_workflow_should_not_build_linux_gnu_targets() {
    let targets = build_targets();
    assert!(
        !targets.contains("x86_64-unknown-linux-gnu"),
        "release workflow must not build x86_64-unknown-linux-gnu"
    );
    assert!(
        !targets.contains("aarch64-unknown-linux-gnu"),
        "release workflow must not build aarch64-unknown-linux-gnu"
    );
}

#[test]
fn release_workflow_should_keep_installer_asset_names() {
    let targets = build_targets();
    assert!(
        targets.contains("aegis-linux-x86_64"),
        "release workflow must keep aegis-linux-x86_64 asset name"
    );
    assert!(
        targets.contains("aegis-linux-aarch64"),
        "release workflow must keep aegis-linux-aarch64 asset name"
    );
}

#[test]
fn release_workflow_should_verify_static_linux_binaries() {
    let wf = release_workflow();
    assert!(
        wf.contains("Verify static Linux binary"),
        "release workflow must include a 'Verify static Linux binary' step"
    );
    assert!(
        wf.contains("unknown-linux-musl"),
        "release workflow must reference unknown-linux-musl in verification"
    );
    assert!(
        wf.contains("readelf"),
        "release workflow must invoke readelf to verify static linkage (ldd is unreliable for static-pie and cross-compiled binaries)"
    );
    assert!(
        wf.contains("INTERP"),
        "release workflow must assert the binary has no dynamic interpreter (PT_INTERP)"
    );
    assert!(
        wf.contains("NEEDED"),
        "release workflow must assert the binary has no shared library dependencies (DT_NEEDED)"
    );
}

#[test]
fn release_workflow_should_build_linux_musl_targets_via_cross() {
    let targets = build_targets();

    for target in ["x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl"] {
        let entry = matrix_entry(&targets, target);
        assert!(
            entry.contains("\"use_cross\": true"),
            "release workflow must build {target} via cross (use_cross: true); matrix entry:\n{entry}"
        );
    }

    let action = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/actions/build-target/action.yml"),
    )
    .expect("build-target action should be readable");
    assert!(
        action.contains("cross build --release --target ${{ inputs.target }}"),
        "the build-target action must build a use_cross target through cross"
    );
}

/// The four installer-facing release assets required by the release-asset gate. Every supported
/// target must produce a binary and a matching `.sha256` sidecar.
fn expected_release_assets() -> [&'static str; 4] {
    [
        "aegis-linux-x86_64",
        "aegis-linux-aarch64",
        "aegis-macos-x86_64",
        "aegis-macos-aarch64",
    ]
}

#[test]
fn release_workflow_should_define_all_supported_asset_names_in_matrix() {
    let targets = build_targets();

    for asset in expected_release_assets() {
        assert!(
            targets.contains(&format!("\"asset_name\": \"{asset}\"")),
            "release workflow matrix must define asset_name: {asset}"
        );
    }
}

#[test]
fn release_workflow_should_upload_binary_and_sha256_for_each_matrix_entry() {
    let wf = release_workflow();

    assert!(
        wf.contains("name: ${{ matrix.asset_name }}"),
        "upload-artifact must name each artifact from matrix.asset_name"
    );
    assert!(
        wf.contains("${{ matrix.asset_name }}.sha256"),
        "upload-artifact path must include each matrix asset's .sha256 sidecar"
    );
    assert!(
        wf.contains("if-no-files-found: error"),
        "upload-artifact must fail closed if any binary or sidecar is missing"
    );
}

#[test]
fn release_workflow_should_publish_each_binary_and_matching_sha256_sidecar() {
    let patterns = release_files_patterns();

    for asset in expected_release_assets() {
        assert!(
            patterns
                .iter()
                .any(|pattern| pattern == &format!("artifacts/{asset}")),
            "GitHub Release files list must publish binary artifact {asset}"
        );
        assert!(
            patterns
                .iter()
                .any(|pattern| pattern == &format!("artifacts/{asset}.sha256")),
            "GitHub Release files list must publish checksum sidecar {asset}.sha256"
        );
    }
}

#[test]
fn release_workflow_should_publish_the_grammar_license_notice() {
    let wf = release_workflow().replace("\r\n", "\n");

    // Asserted against the derived `files:` patterns so a passing mention in a
    // comment or another job cannot satisfy the contract. Unlike its siblings
    // this entry is repo-relative rather than `artifacts/`-prefixed: the notice
    // is checked into the repo, not produced by the build matrix, so the step
    // that derives the list appends it rather than reading it from the matrix.
    assert!(
        release_files_patterns()
            .iter()
            .any(|pattern| pattern == "THIRD_PARTY_NOTICES.md"),
        "GitHub Release files list must publish the grammar license notice"
    );
    assert!(
        wf.contains("echo \"THIRD_PARTY_NOTICES.md\""),
        "the asset list step must append the grammar license notice"
    );
    assert!(
        wf.contains("fail_on_unmatched_files: true"),
        "GitHub Release must fail closed if a published asset path stops matching"
    );
}

#[test]
fn release_workflow_should_use_node24_actions_for_release_publication() {
    let wf = release_workflow();

    for node24_action in [
        "actions/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c # v8.0.1",
        "softprops/action-gh-release@3d0d9888cb7fd7b750713d6e236d1fcb99157228 # v3.0.2",
    ] {
        assert!(
            wf.contains(node24_action),
            "release publication must pin the Node.js 24 action {node24_action}"
        );
    }

    for node20_action in [
        "actions/download-artifact@634f93cb2916e3fdff6788551b99b062d0335ce0",
        "softprops/action-gh-release@3bb12739c298aeb8a4eeaf626c5b8d85266b0e65",
    ] {
        assert!(
            !wf.contains(node20_action),
            "release publication must not retain the Node.js 20 action {node20_action}"
        );
    }
}

#[test]
fn release_workflow_should_publish_the_changelog_section_as_the_release_body() {
    let wf = release_workflow().replace("\r\n", "\n");

    assert!(
        wf.contains("name: Extract release notes from CHANGELOG"),
        "release workflow must derive the Release body from CHANGELOG.md"
    );
    assert!(
        wf.contains("body_path: release-notes.md"),
        "GitHub Release must take its body from the extracted CHANGELOG section"
    );
    assert!(
        wf.contains("generate_release_notes: false"),
        "GitHub Release must not fall back to the auto-generated commit/PR list, \
         which would replace the curated CHANGELOG entries"
    );

    let extract = wf
        .split_once("name: Extract release notes from CHANGELOG")
        .and_then(|(_, rest)| rest.split_once("\n      - name: "))
        .map(|(step, _)| step.to_owned())
        .expect("extraction step must be followed by another step");
    assert!(
        extract.contains("set -euo pipefail"),
        "extraction step must fail on the first error, not continue with a partial body"
    );
    assert!(
        extract.contains("release-notes.md") && extract.contains("exit 1"),
        "extraction step must fail closed when the tag has no CHANGELOG section"
    );
}

/// The Release body is extracted by tag, so a tag can only be cut once its
/// version owns a section in `CHANGELOG.md`.
#[test]
fn changelog_should_carry_a_section_for_the_current_crate_version() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(root.join("Cargo.toml"))
        .expect("workspace manifest should be readable");
    let version = manifest
        .lines()
        .find_map(|line| line.strip_prefix("version = \""))
        .and_then(|rest| rest.split('"').next())
        .expect("workspace manifest should declare a package version");

    let changelog = std::fs::read_to_string(root.join("CHANGELOG.md"))
        .expect("CHANGELOG should be readable")
        .replace("\r\n", "\n");
    let heading = format!("## [{version}]");
    let sections = changelog
        .lines()
        .filter(|line| line.starts_with(&heading))
        .count();

    assert_eq!(
        sections, 1,
        "CHANGELOG.md must carry exactly one `{heading}` section for the released \
         crate version — the release workflow publishes that section verbatim, so a \
         missing section fails the release and a duplicate silently truncates it"
    );
}

#[test]
fn release_workflow_should_generate_sha256_before_uploading_artifacts() {
    let wf = release_workflow();
    let checksum_step = wf
        .find("name: Generate SHA256 checksum")
        .expect("release workflow must generate SHA256 sidecars");
    let upload_step = wf
        .find("name: Upload binary artifact")
        .expect("release workflow must upload binary artifacts");

    assert!(
        checksum_step < upload_step,
        "release workflow must generate SHA256 sidecars before artifact upload"
    );
    assert!(
        wf.contains("sha256sum ${{ matrix.asset_name }} > ${{ matrix.asset_name }}.sha256")
            || wf.contains(
                "shasum -a 256 ${{ matrix.asset_name }} > ${{ matrix.asset_name }}.sha256"
            ),
        "release workflow must write checksum output to <asset>.sha256"
    );
}
