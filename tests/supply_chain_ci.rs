use std::fs;
use std::path::{Path, PathBuf};

fn repo_path(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn security_gate_action() -> String {
    fs::read_to_string(repo_path(".github/actions/security-gate/action.yml"))
        .expect("security-gate composite action should be readable")
}

#[test]
fn security_gate_runs_full_cargo_deny_check() {
    let action = security_gate_action();

    assert!(
        action.contains("cargo deny check"),
        "the security gate must run cargo deny check"
    );
    assert!(
        !action.contains("cargo deny check bans licenses sources"),
        "the security gate must not omit advisories from cargo deny check"
    );
}

#[test]
fn security_gate_runs_cargo_audit() {
    assert!(
        security_gate_action().contains("cargo audit"),
        "the security gate must run cargo audit"
    );
}

// The audit and deny steps live in a composite action so that CI and the
// release workflow run the identical checks. These two assertions are what
// keeps that true: without them, either caller could quietly drop the action
// and the tests above would still pass against an action nobody invokes.
#[test]
fn ci_security_job_invokes_the_security_gate() {
    let workflow = fs::read_to_string(repo_path(".github/workflows/ci.yml"))
        .expect("CI workflow should be readable");

    assert!(
        workflow.contains("./.github/actions/security-gate"),
        "CI must run the supply-chain checks through the security-gate action"
    );
}

#[test]
fn release_workflow_invokes_the_security_gate_before_building() {
    let workflow = fs::read_to_string(repo_path(".github/workflows/release.yml"))
        .expect("release workflow should be readable");

    assert!(
        workflow.contains("./.github/actions/security-gate"),
        "the release workflow must run the supply-chain checks on the tagged commit"
    );
    assert!(
        workflow.contains("needs: [config, admission-quality, admission-security]"),
        "the release build matrix must wait for the Tag admission check"
    );
}

fn ci_workflow() -> String {
    fs::read_to_string(repo_path(".github/workflows/ci.yml"))
        .expect("CI workflow should be readable")
}

fn release_workflow() -> String {
    fs::read_to_string(repo_path(".github/workflows/release.yml"))
        .expect("release workflow should be readable")
}

/// `.github/versions.env` as `(KEY, value)` pairs, comments and blank lines
/// dropped.
fn pinned_versions() -> Vec<(String, String)> {
    let file = fs::read_to_string(repo_path(".github/versions.env"))
        .expect("versions.env should be readable");

    file.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (key, value) = line
                .split_once('=')
                .unwrap_or_else(|| panic!("versions.env line is not KEY=value: {line}"));
            (key.to_string(), value.to_string())
        })
        .collect()
}

fn pinned_version(key: &str) -> String {
    pinned_versions()
        .into_iter()
        .find(|(name, _)| name == key)
        .unwrap_or_else(|| panic!("versions.env should pin {key}"))
        .1
}

#[test]
fn both_workflows_read_the_versions_from_the_one_file() {
    for (name, workflow) in [
        ("ci.yml", ci_workflow()),
        ("release.yml", release_workflow()),
    ] {
        assert!(
            workflow.contains("./.github/actions/load-versions"),
            "{name} must take its pinned versions from .github/versions.env \
             through the load-versions action"
        );
    }
}

/// A second copy of a version is how the release toolchain drifts from the one
/// CI tested with. Every pin belongs in `.github/versions.env` and reaches a
/// job as an output of the `gate`/`config` job.
#[test]
fn no_workflow_hardcodes_a_pinned_version() {
    for (key, value) in pinned_versions() {
        // A dotted or dated version string is distinctive enough to search for
        // on its own. A bare number (a Node major, an iteration count) would
        // collide with unrelated digits inside a pinned action SHA, so it is
        // searched for in the shapes a workflow would actually spell it:
        // `node-version: 22`, `-runs=100000`.
        let spellings: Vec<String> = if value.contains('.') || value.contains('-') {
            vec![value.clone()]
        } else {
            vec![
                format!("={value}"),
                format!(": {value}"),
                format!(": \"{value}\""),
            ]
        };

        for (name, workflow) in [
            ("ci.yml", ci_workflow()),
            ("release.yml", release_workflow()),
        ] {
            for spelling in &spellings {
                assert!(
                    !workflow.contains(spelling),
                    "{name} writes {key}={value} a second time (as `{spelling}`); \
                     read it from .github/versions.env instead"
                );
            }
        }
    }
}

#[test]
fn docs_ci_documents_the_pinned_versions() {
    let docs = fs::read_to_string(repo_path("docs/ci.md")).expect("docs/ci.md should be readable");

    let documented = [
        format!("- Rust toolchain: `{}`", pinned_version("RUST_TOOLCHAIN")),
        format!(
            "- `cargo-audit`: `{}`",
            pinned_version("CARGO_AUDIT_VERSION")
        ),
        format!("- `cargo-deny`: `{}`", pinned_version("CARGO_DENY_VERSION")),
        format!("- `cross`: `{}`", pinned_version("CROSS_VERSION")),
    ];

    for line in documented {
        assert!(
            docs.contains(&line),
            "docs/ci.md should document the pinned inputs verbatim; missing: {line}"
        );
    }
}

#[test]
fn docs_ci_documents_every_release_target() {
    let targets = fs::read_to_string(repo_path(".github/build-targets.json"))
        .expect("build-targets.json should be readable");
    let docs = fs::read_to_string(repo_path("docs/ci.md")).expect("docs/ci.md should be readable");

    let mut found_any = false;
    for fragment in targets.split("\"target\": \"").skip(1) {
        let triple = fragment
            .split('"')
            .next()
            .expect("a quoted target triple should be terminated");
        found_any = true;
        assert!(
            docs.contains(triple),
            "docs/ci.md should list the {triple} release target"
        );
    }

    assert!(
        found_any,
        "build-targets.json should define at least one release target"
    );
}

/// Every `actions/checkout` step in a workflow, as the block of YAML from the
/// `uses:` line up to the next step in the same job.
fn checkout_steps(workflow: &str) -> Vec<String> {
    workflow
        .split("actions/checkout@")
        .skip(1)
        .map(|rest| {
            let end = rest.find("\n      - ").unwrap_or(rest.len());
            rest[..end].to_string()
        })
        .collect()
}

/// `actions/checkout` writes the workflow token into `.git/config` by default,
/// where any later step, build script, or dependency can read it. No job here
/// pushes with it: the release notes are published by `action-gh-release` with
/// its own token, and the tag admission check talks to the compare API through
/// `GH_TOKEN`.
#[test]
fn no_checkout_leaves_the_workflow_token_in_the_checkout() {
    for (name, workflow) in [
        ("ci.yml", ci_workflow()),
        ("release.yml", release_workflow()),
    ] {
        let steps = checkout_steps(&workflow);
        assert!(
            !steps.is_empty(),
            "{name} should check the repository out at least once"
        );

        for (index, step) in steps.iter().enumerate() {
            assert!(
                step.contains("persist-credentials: false"),
                "{name} checkout #{} keeps the workflow token in .git/config; \
                 set persist-credentials: false",
                index + 1
            );
        }
    }
}

/// An empty `targets` array is valid JSON and would sail through: the build
/// matrix would expand to nothing, and the Release would publish with only
/// `THIRD_PARTY_NOTICES.md` attached. Both readers must count the targets.
#[test]
fn an_empty_target_table_stops_the_build_and_the_release() {
    let loader = fs::read_to_string(repo_path(".github/actions/load-versions/action.yml"))
        .expect("load-versions action should be readable");
    assert!(
        loader.contains("jq '.targets | length'"),
        "load-versions must reject a build-targets.json with no targets"
    );

    assert!(
        release_workflow().contains("jq '.targets | length'"),
        "the release asset list must reject a build-targets.json with no targets"
    );
}
