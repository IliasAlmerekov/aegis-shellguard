//! `scripts/lint.sh` is the only place the rustfmt and clippy invocations are
//! written (#276). CI, the pre-push hook and `just lint` call the script, and
//! the contributor docs point at it. Before this, the three callers carried
//! their own copies of the command, the copies drifted apart, and CI stopped
//! linting tests without anyone noticing.

use std::fs;
use std::path::{Path, PathBuf};

const LINT_SCRIPT: &str = "scripts/lint.sh";

/// Files that run the lint checks. Each one must go through the script.
const CALLERS: &[&str] = &[
    ".github/actions/quality-gate/action.yml",
    ".githooks/pre-push",
    "justfile",
];

/// Files that tell a person or an agent how to run the lint checks.
const DOCS: &[&str] = &[
    "CONTRIBUTING.md",
    "CONVENTION.md",
    "AGENTS.md",
    ".github/pull_request_template.md",
    "docs/ci.md",
];

fn repo_path(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn read(path: &str) -> String {
    fs::read_to_string(repo_path(path)).unwrap_or_else(|err| panic!("{path} unreadable: {err}"))
}

#[test]
fn lint_script_runs_clippy_on_every_target_and_feature() {
    let script = read(LINT_SCRIPT);

    assert!(
        script.contains("clippy --workspace --all-targets --all-features --locked -- -D warnings"),
        "{LINT_SCRIPT} must lint tests, benches, binaries and opt-in features"
    );
    assert!(
        script.contains("fmt --check --all"),
        "{LINT_SCRIPT} must check formatting for every workspace member"
    );
}

#[test]
fn lint_script_takes_the_toolchain_from_versions_env() {
    let script = read(LINT_SCRIPT);

    assert!(
        script.contains(".github/versions.env") && script.contains("RUST_TOOLCHAIN"),
        "{LINT_SCRIPT} must read the pinned toolchain, not the local default"
    );
}

#[test]
fn every_caller_runs_the_lint_script() {
    for caller in CALLERS {
        assert!(
            read(caller).contains(LINT_SCRIPT),
            "{caller} must run the lint checks through {LINT_SCRIPT}"
        );
    }
}

#[test]
fn no_caller_or_doc_keeps_its_own_copy_of_the_command() {
    for path in CALLERS.iter().chain(DOCS) {
        let text = read(path);
        for copy in ["cargo clippy", "cargo fmt --check"] {
            assert!(
                !text.contains(copy),
                "{path} contains `{copy}`; point it at {LINT_SCRIPT} instead"
            );
        }
    }
}
