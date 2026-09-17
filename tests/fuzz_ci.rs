use std::path::{Path, PathBuf};

fn repo_path(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn read_repo_file(path: &str) -> String {
    std::fs::read_to_string(repo_path(path))
        .unwrap_or_else(|error| panic!("{path} should be readable: {error}"))
}

fn assert_fuzz_target_declared(manifest: &str, target: &str) {
    assert!(
        manifest.contains(&format!("name = \"{target}\"")),
        "fuzz/Cargo.toml must declare fuzz target {target}"
    );
    assert!(
        manifest.contains(&format!("path = \"fuzz_targets/{target}.rs\"")),
        "fuzz/Cargo.toml must point {target} at fuzz_targets/{target}.rs"
    );
}

/// The target list the CI fuzz step loops over, i.e. the words between
/// `for target in` and the `; do` that closes the list. Line continuations are
/// dropped so a wrapped list reads the same as a single-line one.
///
/// Panics if the loop is absent — a test-fixture failure, not a runtime one.
fn ci_fuzz_targets(ci: &str) -> Vec<String> {
    let list = ci
        .split_once("for target in ")
        .and_then(|(_, rest)| rest.split_once("; do"))
        .map(|(list, _)| list.to_owned())
        .expect("the CI fuzz step should loop over a target list");

    list.replace('\\', " ")
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// The iteration count the fuzz step passes to libFuzzer, pinned in
/// `.github/versions.env` rather than in the workflow.
fn ci_fuzz_runs() -> u64 {
    read_repo_file(".github/versions.env")
        .lines()
        .find_map(|line| line.trim().strip_prefix("FUZZ_RUNS="))
        .expect("versions.env should pin FUZZ_RUNS")
        .trim()
        .parse()
        .expect("FUZZ_RUNS should be a number")
}

#[test]
fn fuzz_manifest_declares_all_prd_targets() {
    let manifest = read_repo_file("fuzz/Cargo.toml");

    for target in [
        "parser",
        "scanner",
        "heredoc",
        "language_protocol",
        "router",
        "language_python",
        "language_javascript",
        "language_typescript",
        "language_bash",
    ] {
        assert_fuzz_target_declared(&manifest, target);
    }
}

#[test]
fn ci_runs_each_fuzz_target_for_at_least_100000_iterations() {
    let ci = read_repo_file(".github/workflows/ci.yml");
    let looped = ci_fuzz_targets(&ci);

    for target in [
        "parser",
        "scanner",
        "heredoc",
        "language_protocol",
        "router",
        "language_python",
        "language_javascript",
        "language_typescript",
        "language_bash",
    ] {
        assert!(
            looped.iter().any(|looped| looped == target),
            "the CI fuzz step must run {target}; it loops over {looped:?}"
        );
    }

    assert!(
        ci.contains("fuzz run \"$target\" \"fuzz/corpus/$target\""),
        "the CI fuzz step must seed every target from its committed corpus"
    );
    assert!(
        ci.contains("\"-runs=$RUNS\""),
        "the CI fuzz step must bound every target by the pinned iteration count"
    );
    assert!(
        ci_fuzz_runs() >= 100_000,
        "versions.env must keep FUZZ_RUNS at 100000 iterations or more"
    );
}

#[test]
fn fuzz_corpora_are_committed_for_all_prd_targets() {
    for target in [
        "parser",
        "scanner",
        "heredoc",
        "language_protocol",
        "router",
        "language_python",
        "language_javascript",
        "language_typescript",
        "language_bash",
    ] {
        let corpus_dir = repo_path(&format!("fuzz/corpus/{target}"));
        let entries: Vec<_> = std::fs::read_dir(&corpus_dir)
            .unwrap_or_else(|error| {
                panic!(
                    "fuzz corpus directory {} should be readable: {error}",
                    corpus_dir.display()
                )
            })
            .collect::<Result<_, _>>()
            .unwrap_or_else(|error| {
                panic!(
                    "fuzz corpus directory {} should not contain unreadable entries: {error}",
                    corpus_dir.display()
                )
            });

        assert!(
            entries
                .iter()
                .any(|entry| entry.file_type().is_ok_and(|kind| kind.is_file())),
            "fuzz/corpus/{target} must contain at least one committed seed file"
        );
    }
}
