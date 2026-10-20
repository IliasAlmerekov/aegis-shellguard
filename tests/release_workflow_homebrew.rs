//! Regression tests for the Homebrew tap jobs of the release workflow.
//!
//! Split out of `tests/release_workflow.rs` to keep both files under the
//! file-size budget. The publication tests run the workflow's real
//! "Commit and push the formula" step against a local bare tap.

use std::path::Path;
use std::process::{Command, Output};

fn release_workflow() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release.yml");
    std::fs::read_to_string(&path).expect("release workflow should be readable")
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

const RELEASE_URL: &str = "https://github.com/IliasAlmerekov/aegis-shellguard/releases/download";

/// A generated formula: the version lives only in the release URLs.
fn formula_fixture(version: &str) -> String {
    format!(
        "class Aegis < Formula\n  \
         url \"{RELEASE_URL}/v{version}/aegis-linux-x86_64\", using: :nounzip\n  \
         resource \"third_party_notices\" do\n    \
         url \"{RELEASE_URL}/v{version}/THIRD_PARTY_NOTICES.md\"\n  end\nend\n"
    )
}

/// The formula published up to v0.7.1: an explicit `version` line plus URLs.
fn legacy_formula_fixture(version: &str) -> String {
    formula_fixture(version).replacen(
        "class Aegis < Formula\n",
        &format!("class Aegis < Formula\n  version \"{version}\"\n"),
        1,
    )
}

fn local_tap(version: &str) -> tempfile::TempDir {
    local_tap_with(&formula_fixture(version))
}

fn local_tap_with(formula: &str) -> tempfile::TempDir {
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
    std::fs::write(tap.join("Formula/aegis.rb"), formula).unwrap();
    tap_git(&tap, &["add", "Formula/aegis.rb"]);
    tap_git(&tap, &["commit", "-m", "Initial formula"]);
    tap_git(&tap, &["push", "origin", "main"]);
    std::fs::create_dir(root.join("formula")).unwrap();
    directory
}

fn publish_to_local_tap(root: &Path, version: &str) -> Output {
    publish_formula(root, version, &formula_fixture(version))
}

fn publish_formula(root: &Path, version: &str, formula: &str) -> Output {
    std::fs::write(root.join("formula/aegis.rb"), formula).unwrap();
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

#[test]
fn homebrew_publication_should_refuse_a_downgrade_from_a_legacy_version_line_formula() {
    let directory = local_tap_with(&legacy_formula_fixture("0.7.1"));
    let root = directory.path();
    let remote = root.join("remote.git");
    let published = tap_git(&remote, &["rev-parse", "main"]);

    let older = publish_to_local_tap(root, "0.7.0");
    assert!(!older.status.success(), "{older:?}");
    assert_eq!(tap_git(&remote, &["rev-parse", "main"]), published);
}

#[test]
fn homebrew_publication_should_upgrade_a_legacy_version_line_formula() {
    let directory = local_tap_with(&legacy_formula_fixture("0.7.1"));
    let root = directory.path();

    let newer = publish_to_local_tap(root, "0.7.2");
    assert!(newer.status.success(), "{newer:?}");
    assert_eq!(
        tap_git(&root.join("remote.git"), &["show", "main:Formula/aegis.rb"]),
        formula_fixture("0.7.2")
    );
}

#[test]
fn homebrew_publication_should_reject_urls_that_disagree_on_the_version() {
    let mixed = formula_fixture("0.7.2").replacen("v0.7.2", "v0.7.1", 1);
    let directory = local_tap("0.7.0");
    let root = directory.path();
    let remote = root.join("remote.git");
    let published = tap_git(&remote, &["rev-parse", "main"]);

    let output = publish_formula(root, "0.7.2", &mixed);
    assert!(!output.status.success(), "{output:?}");
    assert_eq!(tap_git(&remote, &["rev-parse", "main"]), published);
}

#[test]
fn homebrew_publication_should_reject_a_generated_formula_for_another_tag() {
    let directory = local_tap("0.7.0");
    let root = directory.path();

    let output = publish_formula(root, "0.7.2", &formula_fixture("0.7.1"));
    assert!(!output.status.success(), "{output:?}");
}

#[test]
fn homebrew_publication_should_reject_a_formula_url_outside_the_release_downloads() {
    let foreign = formula_fixture("0.7.2").replace(RELEASE_URL, "https://example.invalid/dl");
    let directory = local_tap("0.7.0");

    let output = publish_formula(directory.path(), "0.7.2", &foreign);
    assert!(!output.status.success(), "{output:?}");
}
