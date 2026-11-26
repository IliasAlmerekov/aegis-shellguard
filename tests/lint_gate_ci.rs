#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail loudly"
)]
mod support;

use std::fs;

use support::repo_path;

const LINT_SCRIPT: &str = "scripts/lint.sh";

const CALLERS: &[&str] = &[
    ".github/actions/quality-gate/action.yml",
    ".githooks/pre-push",
    "justfile",
];

const DOCS: &[&str] = &[
    "CONTRIBUTING.md",
    "CONVENTION.md",
    "AGENTS.md",
    ".github/pull_request_template.md",
    "docs/ci.md",
];

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
