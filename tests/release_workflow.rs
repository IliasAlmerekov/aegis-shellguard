//! Regression tests for static-musl release targets.
//!
//! These tests encode the release-workflow target matrix contract: both Linux
//! musl targets build through `cross`, and static-binary verification runs
//! before checksum generation. The asset-name test preserves the installer
//! asset contract.

use std::path::Path;
use std::process::{Command, Output};

fn release_workflow() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release.yml");
    std::fs::read_to_string(&path).expect("release workflow should be readable")
}

#[test]
fn npm_publish_should_use_trusted_publishing_without_a_registry_token() {
    let workflow = release_workflow();
    let publish = workflow
        .split_once("  publish-npm:")
        .map(|(_, job)| job)
        .expect("release workflow should contain the npm publish job");

    assert!(
        publish.contains("id-token: write"),
        "npm publish needs an OIDC token"
    );
    assert!(
        publish.contains("npm install --global npm@${NPM_CLI_VERSION}"),
        "npm publish needs the pinned OIDC-capable npm CLI"
    );
    assert!(
        !publish.contains("secrets.NPM_TOKEN") && !publish.contains("NODE_AUTH_TOKEN"),
        "npm publish must not fall back to a long-lived registry token"
    );
    assert!(
        publish.contains("NPM_CONFIG_PROVENANCE: ${{ github.event_name == 'workflow_dispatch' && 'false' || 'true' }}"),
        "a recovery dispatch must not attribute the tagged package to main's workflow commit"
    );
}

#[test]
fn npm_publish_recovery_should_checkout_and_validate_the_released_tag() {
    let workflow = release_workflow();
    let publish = workflow
        .split_once("  publish-npm:")
        .map(|(_, job)| job)
        .expect("release workflow should contain the npm publish job");

    assert!(workflow.contains("workflow_dispatch:"));
    assert!(publish.contains("$GITHUB_REF\" != \"refs/heads/main"));
    assert!(publish.contains("ref: ${{ steps.target.outputs.sha }}"));
    assert!(publish.contains("sha=$SHA"));
    assert!(publish.contains("compare/main...$SHA"));
    assert!(publish.contains("gh release view \"$TAG\""));
    assert!(
        publish.contains("scripts/update-npm-package.sh \"$TAG\""),
        "npm checksums must come from the selected published release"
    );
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

/// The major version of every pin of `action` in the workflow, read from the
/// `# vX.Y.Z` comment that follows the SHA.
///
/// Third-party actions are pinned by commit SHA, so the trailing tag comment is
/// the only readable version in the file. Dependabot rewrites the SHA and the
/// comment together, which is why the version floor below is asserted against
/// the comment instead of against a literal SHA: a patch bump stays green while
/// a downgrade across the major boundary still fails.
fn pinned_action_majors(wf: &str, action: &str) -> Vec<u32> {
    wf.split(&format!("{action}@"))
        .skip(1)
        .filter_map(|rest| rest.split('\n').next())
        .filter_map(|line| line.split_once("# v"))
        .filter_map(|(_, version)| version.split('.').next())
        .filter_map(|major| major.trim().parse().ok())
        .collect()
}

#[test]
fn release_workflow_should_use_node24_actions_for_release_publication() {
    let wf = release_workflow();

    // Each floor is the major that replaced a Node.js 20 pin in #128. Dropping
    // below it brings the Node.js 20 runtime back, so the floor is the
    // contract and the exact patch release is Dependabot's business.
    for (action, node24_major) in [
        ("actions/download-artifact", 8),
        ("softprops/action-gh-release", 3),
    ] {
        let majors = pinned_action_majors(&wf, action);
        assert!(
            !majors.is_empty(),
            "release publication must pin {action} by SHA with a # vX.Y.Z tag comment"
        );
        for major in majors {
            assert!(
                major >= node24_major,
                "release publication must pin {action} at v{node24_major} or later \
                 to keep the Node.js 24 runtime, found v{major}"
            );
        }
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

/// The text of one top-level job, from its `  <name>:` line to the next job
/// or the comment block that introduces it.
fn job(workflow: &str, name: &str) -> String {
    let header = format!("  {name}:");
    let mut lines = workflow.lines().skip_while(|line| *line != header);
    let first = lines
        .next()
        .unwrap_or_else(|| panic!("release workflow should contain the {name} job"));
    let mut body = vec![first];
    for line in lines {
        let top_level = line.starts_with("  ") && !line.starts_with("   ");
        if top_level && !line.trim().is_empty() {
            break;
        }
        body.push(line);
    }
    body.join("\n")
}

#[test]
fn homebrew_generate_job_should_run_after_release_without_the_tap_key_or_cargo() {
    let wf = release_workflow();
    let generate = job(&wf, "generate-homebrew-formula");

    assert!(generate.contains("needs: [config, release]"));
    assert!(generate.contains("scripts/update-homebrew-formula.sh"));
    assert!(generate.contains("actions/upload-artifact@"));
    assert!(generate.contains("if-no-files-found: error"));
    assert!(generate.contains("persist-credentials: false"));
    assert!(
        !generate.contains("HOMEBREW_TAP_DEPLOY_KEY"),
        "the generate job must not hold the tap key"
    );
    assert!(
        !generate.contains("cargo") && !generate.contains("setup-rust"),
        "the generate job runs no cargo step"
    );
}

#[test]
fn homebrew_jobs_should_skip_prereleases_and_manual_dispatch() {
    let wf = release_workflow();

    for name in ["generate-homebrew-formula", "publish-homebrew-tap"] {
        let body = job(&wf, name);
        for suffix in ["-rc", "-beta", "-alpha"] {
            assert!(
                body.contains(&format!("!contains(github.ref_name, '{suffix}')")),
                "{name} must skip {suffix} tags"
            );
        }
        assert!(
            body.contains("github.event_name == 'push'"),
            "{name} must run on tag pushes only, not workflow_dispatch"
        );
        assert!(
            !body.contains("github.event_name == 'workflow_dispatch'"),
            "{name} must not run on workflow_dispatch"
        );
    }
}

#[test]
fn homebrew_push_job_should_follow_generate_and_use_the_deploy_key_without_cargo() {
    let wf = release_workflow();
    let push = job(&wf, "publish-homebrew-tap");

    assert!(
        push.contains("generate-homebrew-formula"),
        "the push job must need the generate job"
    );
    assert!(
        wf.find("\n  generate-homebrew-formula:") < wf.find("\n  publish-homebrew-tap:"),
        "the push job must be declared after the generate job"
    );
    assert!(push.contains("repository: IliasAlmerekov/homebrew-aegis"));
    assert!(push.contains("ssh-key: ${{ secrets.HOMEBREW_TAP_DEPLOY_KEY }}"));
    assert!(push.contains("actions/download-artifact@"));
    assert!(push.contains("git diff --cached --quiet"));
    assert!(push.contains("contents: read"));
    assert!(
        !push.contains("cargo") && !push.contains("setup-rust"),
        "the push job runs no cargo or Rust setup"
    );
}

#[test]
fn homebrew_deploy_key_should_only_appear_in_the_push_job() {
    let wf = release_workflow();
    let push = job(&wf, "publish-homebrew-tap");

    assert_eq!(
        wf.matches("HOMEBREW_TAP_DEPLOY_KEY").count(),
        push.matches("HOMEBREW_TAP_DEPLOY_KEY").count(),
        "only the push job may reference the tap deploy key"
    );
}

// Execute the workflow's actual publication step, not a test-only copy.
fn homebrew_publish_step() -> String {
    let workflow = release_workflow().replace("\r\n", "\n");
    let publish = job(&workflow, "publish-homebrew-tap");
    publish
        .split_once("      - name: Commit and push the formula\n")
        .and_then(|(_, step)| step.split_once("        run: |\n"))
        .expect("publication step should have a bash run block")
        .1
        .lines()
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn tap_git(directory: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(directory)
        .args(arguments)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .expect("git should start");
    assert!(output.status.success(), "git {arguments:?}: {output:?}");
    String::from_utf8(output.stdout).expect("git output should be UTF-8")
}

fn formula_fixture(version: &str) -> String {
    format!("class Aegis < Formula\n  version \"{version}\"\nend\n")
}

fn local_tap(version: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("temporary tap should be created");
    let root = directory.path();
    tap_git(
        root,
        &["init", "--bare", "--initial-branch=main", "remote.git"],
    );
    tap_git(root, &["clone", "remote.git", "tap"]);
    let tap = root.join("tap");
    tap_git(&tap, &["config", "user.name", "Release test"]);
    tap_git(
        &tap,
        &["config", "user.email", "release-test@example.invalid"],
    );
    std::fs::create_dir(tap.join("Formula")).unwrap();
    std::fs::write(tap.join("Formula/aegis.rb"), formula_fixture(version)).unwrap();
    tap_git(&tap, &["add", "Formula/aegis.rb"]);
    tap_git(&tap, &["commit", "-m", "Initial formula"]);
    tap_git(&tap, &["push", "origin", "main"]);
    std::fs::create_dir(root.join("formula")).unwrap();
    directory
}

fn publish_to_local_tap(root: &Path, version: &str) -> Output {
    std::fs::write(root.join("formula/aegis.rb"), formula_fixture(version)).unwrap();
    Command::new("bash")
        .args(["-c", &homebrew_publish_step()])
        .current_dir(root)
        .env("TAG", format!("v{version}"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .expect("publication step should start")
}

#[test]
fn homebrew_old_release_recovery_should_not_downgrade_the_tap() {
    let directory = local_tap("0.7.0");
    let root = directory.path();
    let newer = publish_to_local_tap(root, "0.7.2");
    assert!(newer.status.success(), "{newer:?}");
    let remote = root.join("remote.git");
    let published = tap_git(&remote, &["rev-parse", "main"]);

    let older = publish_to_local_tap(root, "0.7.1");
    assert!(!older.status.success(), "old release must fail: {older:?}");
    assert_eq!(tap_git(&remote, &["rev-parse", "main"]), published);
    assert_eq!(
        std::fs::read_to_string(root.join("tap/Formula/aegis.rb")).unwrap(),
        formula_fixture("0.7.2"),
        "downgrade must be rejected before copying the formula"
    );
}

#[test]
fn homebrew_republication_should_not_create_another_commit() {
    let directory = local_tap("0.7.0");
    let root = directory.path();
    let first = publish_to_local_tap(root, "0.7.1");
    assert!(first.status.success(), "{first:?}");
    let remote = root.join("remote.git");
    assert_eq!(
        tap_git(&remote, &["log", "-1", "--format=%s"]),
        "aegis 0.7.1\n"
    );
    assert_eq!(
        tap_git(&remote, &["show", "main:Formula/aegis.rb"]),
        formula_fixture("0.7.1")
    );
    let published = tap_git(&remote, &["rev-parse", "main"]);

    let repeated = publish_to_local_tap(root, "0.7.1");
    assert!(repeated.status.success(), "{repeated:?}");
    assert_eq!(tap_git(&remote, &["rev-parse", "main"]), published);
    assert_eq!(tap_git(&remote, &["rev-list", "--count", "main"]), "2\n");
}

#[test]
fn homebrew_publication_should_compare_stable_versions_numerically() {
    for (current, incoming) in [
        ("0.7.9", "0.7.10"),
        ("0.9.9", "0.10.0"),
        ("0.99.99", "1.0.0"),
    ] {
        let directory = local_tap(current);
        let root = directory.path();
        let output = publish_to_local_tap(root, incoming);
        assert!(
            output.status.success(),
            "{current} -> {incoming}: {output:?}"
        );
        assert_eq!(
            tap_git(&root.join("remote.git"), &["show", "main:Formula/aegis.rb"]),
            formula_fixture(incoming)
        );
    }
}

#[test]
fn homebrew_publication_should_fail_closed_on_an_invalid_current_version() {
    for current in ["unknown", "0.7.2-rc.1", "0.07.2"] {
        let directory = local_tap(current);
        let root = directory.path();
        let remote = root.join("remote.git");
        let published = tap_git(&remote, &["rev-parse", "main"]);
        let output = publish_to_local_tap(root, "0.7.1");
        assert!(!output.status.success(), "{current}: {output:?}");
        assert_eq!(tap_git(&remote, &["rev-parse", "main"]), published);
        assert_eq!(
            std::fs::read_to_string(root.join("tap/Formula/aegis.rb")).unwrap(),
            formula_fixture(current)
        );
    }
}

#[test]
fn homebrew_concurrent_publication_should_not_overwrite_a_newer_remote_commit() {
    let directory = local_tap("0.7.0");
    let root = directory.path();
    std::fs::create_dir(root.join("stale")).unwrap();
    tap_git(root, &["clone", "remote.git", "stale/tap"]);
    std::fs::create_dir(root.join("stale/formula")).unwrap();
    let newer = publish_to_local_tap(root, "0.7.2");
    assert!(newer.status.success(), "{newer:?}");
    let remote = root.join("remote.git");
    let published = tap_git(&remote, &["rev-parse", "main"]);

    let raced = publish_to_local_tap(&root.join("stale"), "0.7.1");
    assert!(
        !raced.status.success(),
        "stale checkout must not push: {raced:?}"
    );
    assert!(String::from_utf8_lossy(&raced.stderr).contains("[rejected]"));
    assert_eq!(tap_git(&remote, &["rev-parse", "main"]), published);
}
