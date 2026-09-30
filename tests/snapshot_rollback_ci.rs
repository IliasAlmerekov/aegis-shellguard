use std::path::{Path, PathBuf};

fn repo_path(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn read_repo_file(path: &str) -> String {
    std::fs::read_to_string(repo_path(path))
        .unwrap_or_else(|error| panic!("{path} should be readable: {error}"))
}

fn workflow() -> String {
    read_repo_file(".github/workflows/ci.yml").replace("\r\n", "\n")
}

#[test]
fn ci_defines_live_snapshot_rollback_job() {
    let ci = workflow();

    assert!(
        ci.contains("snapshot-rollback-live:"),
        "CI must define the live snapshot-rollback job"
    );
    assert!(
        ci.contains("name: Live snapshot/rollback (Docker + SQLite)"),
        "live snapshot-rollback job must have a clear human-readable name"
    );
    assert!(
        ci.contains("runs-on: ubuntu-latest"),
        "live snapshot-rollback job should run on ubuntu-latest where Docker and sqlite3 are available"
    );
}

#[test]
fn ci_live_snapshot_rollback_job_prepares_real_backends() {
    let ci = workflow();

    assert!(
        ci.contains("docker pull alpine"),
        "Docker live tests must pull the alpine fixture image explicitly"
    );
    assert!(
        ci.contains("sudo apt-get install -y sqlite3"),
        "SQLite live tests must install the real sqlite3 CLI"
    );
}

#[test]
fn ci_live_snapshot_rollback_job_runs_docker_and_sqlite_tests() {
    let ci = workflow();

    assert!(
        ci.contains("AEGIS_DOCKER_TESTS: \"1\""),
        "Docker live tests must opt in with AEGIS_DOCKER_TESTS=1"
    );
    assert!(
        ci.contains(
            "cargo test --test docker_integration snapshot_rollback_reverts_filesystem_change -- --exact --nocapture"
        ),
        "CI must run the real Docker snapshot rollback lifecycle test"
    );
    assert!(
        ci.contains("AEGIS_SQLITE_SNAPSHOT_TESTS: \"1\""),
        "SQLite live tests must opt in with AEGIS_SQLITE_SNAPSHOT_TESTS=1"
    );
    assert!(
        ci.contains(
            "cargo test --test snapshot_rollback_live sqlite_snapshot_rollback_restores_database_file_through_aegis_cli -- --exact --nocapture"
        ),
        "CI must run the real SQLite Aegis CLI snapshot rollback lifecycle test"
    );
}

#[test]
fn ci_runs_live_sqlite_snapshot_rollback_on_macos() {
    let ci = workflow();
    let block = ci
        .split("\n  snapshot-rollback-live-macos:\n")
        .nth(1)
        .expect("CI must define the snapshot-rollback-live-macos job")
        .split("\n  fuzz:\n")
        .next()
        .unwrap();

    assert!(
        block.contains("runs-on: ${{ needs.gate.outputs.macos_runner }}"),
        "the macOS live job must use the pinned macOS runner label"
    );
    assert!(
        block.contains("AEGIS_SQLITE_SNAPSHOT_TESTS: \"1\"")
            && block.contains(
                "cargo test --test snapshot_rollback_live sqlite_snapshot_rollback_restores_database_file_through_aegis_cli -- --exact --nocapture"
            ),
        "the macOS live job must run the same SQLite lifecycle test as the Linux job"
    );
    assert!(
        !block.contains("AEGIS_DOCKER_TESTS") && !block.contains("docker "),
        "hosted macOS runners have no Docker daemon, so the macOS job must not run Docker tests"
    );
}

#[test]
fn ci_docs_record_macos_docker_skip() {
    let docs = read_repo_file("docs/ci.md");

    assert!(
        docs.contains("Live snapshot/rollback (SQLite, macOS)"),
        "docs/ci.md must list the macOS live snapshot/rollback job"
    );
    assert!(
        docs.contains("no Docker daemon"),
        "docs/ci.md must record why the Docker live test is skipped on macOS"
    );
}
