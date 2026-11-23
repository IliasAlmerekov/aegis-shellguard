//! Shared helpers for the integration test crates: the `aegis` binary and its
//! environment, audit reading, workspace manifests and git repositories. They
//! are test-only and panic on internal errors (`.unwrap()` is acceptable here).
//
// Each integration test file is compiled as its own crate and includes
// `mod support;`, so only a subset of these helpers is referenced by any
// given crate. Silence dead-code warnings for the unused subset rather than
// forcing every test file to touch every helper.
#![allow(dead_code)]

pub mod agent_hooks;
pub mod installer;
#[path = "../../src/test_git.rs"]
mod test_git;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use aegis_audit::{AuditEntry, AuditLogger, AuditSnapshot};
use aegis_types::{Assessment, Decision, ParsedCommand, RiskLevel};

pub(crate) use test_git::git_command;

/// Runs `git args` in `repo` and returns its output; panics unless git succeeds.
pub fn run_git(repo: &Path, args: &[&str]) -> Output {
    let output = git_command(repo)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("run git {args:?}: {error}"));
    assert!(
        output.status.success(),
        "git {args:?} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

/// Runs `git args` in `repo` and returns its trimmed stdout; panics unless git succeeds.
pub fn git(repo: &Path, args: &[&str]) -> String {
    String::from_utf8_lossy(&run_git(repo, args).stdout)
        .trim()
        .to_string()
}

/// Returns the commit hashes of every stash entry in `repo`.
pub fn stash_hashes(repo: &Path) -> Vec<String> {
    git(repo, &["stash", "list", "--format=%H"])
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Makes `repo` a git repository with a test identity and commits whatever it
/// holds, so the repository always has a first commit.
pub fn init_git_repo(repo: &Path) {
    run_git(repo, &["init", "-q"]);
    run_git(repo, &["config", "user.email", "test@aegis.dev"]);
    run_git(repo, &["config", "user.name", "Aegis Test"]);
    run_git(repo, &["config", "core.autocrlf", "false"]);
    run_git(repo, &["add", "-A"]);
    run_git(repo, &["commit", "-q", "--allow-empty", "-m", "init"]);
}

/// Creates a git repository under `home/repo` with one untracked-file stash
/// and returns its canonicalized path and the stash hash.
pub fn repo_with_stash(home: &Path) -> (PathBuf, String) {
    let repo = home.join("repo");
    fs::create_dir(&repo).unwrap();
    let repo = repo.canonicalize().unwrap();
    init_git_repo(&repo);
    fs::write(repo.join("stashed.txt"), "stashed\n").unwrap();
    git(
        &repo,
        &[
            "stash",
            "push",
            "-q",
            "--include-untracked",
            "-m",
            "aegis-test-stash",
        ],
    );
    let hash = git(&repo, &["rev-parse", "stash@{0}"]);
    (repo, hash)
}

/// Resolve a path relative to the repository root via the test crate's
/// `CARGO_MANIFEST_DIR` (the workspace root for the `aegis` test harness).
#[must_use]
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Resolve a path relative to the repository root.
#[must_use]
pub fn repo_path(relative: &str) -> PathBuf {
    repo_root().join(relative)
}

/// Read a workspace member's `Cargo.toml` and return its `[dependencies]`
/// section onward as a single lowercased string for substring checks. Extracting
/// from `[dependencies]` onward avoids matching the `[package]` description.
#[must_use]
pub fn crate_deps_section(crate_name: &str) -> String {
    let path = repo_root()
        .join("crates")
        .join(crate_name)
        .join("Cargo.toml");
    let content = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    let lower = content.to_lowercase();
    lower
        .find("[dependencies]")
        .map_or(String::new(), |start| lower[start..].to_string())
}

/// Assert that a workspace crate does NOT list a forbidden dependency.
pub fn assert_no_dep(crate_name: &str, forbidden: &str) {
    let deps = crate_deps_section(crate_name);
    assert!(
        !deps.contains(&forbidden.to_lowercase()),
        "architecture boundary violated: `{crate_name}` must not depend on `{forbidden}` \
         (dependency DAG enforced by tests/architecture_boundaries.rs and \
         tests/aegis_language_boundary.rs)"
    );
}

pub fn aegis_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_aegis"))
}

pub fn base_command(home: &Path) -> Command {
    let mut command = Command::new(aegis_bin());
    command.env("AEGIS_REAL_SHELL", "/bin/sh");
    // These end-to-end tests exercise the normal interactive/non-interactive
    // product flow, not the CI fast-path. Force CI detection off so host
    // environments like GitHub Actions do not change the expected exit codes.
    command.env("AEGIS_CI", "0");
    command.env("HOME", home);
    command
}

pub fn direct_shell_command(home: &Path) -> Command {
    let mut command = Command::new("/bin/sh");
    command.env("HOME", home);
    command
}

/// How many times [`until_analysis_completes`] runs a CLI fixture.
pub const ANALYSIS_ATTEMPTS: usize = 5;

/// Runs `attempt` until `completed` holds for its result, at most
/// [`ANALYSIS_ATTEMPTS`] times, and returns that result.
///
/// The CLI clamps the language-analysis deadline to 100 ms, and a loaded
/// `cargo test --workspace` run can miss it (#458). Use this only for a
/// fixture that never executes its command, so a repeat has no side effect.
pub fn until_analysis_completes<T>(
    label: &str,
    mut attempt: impl FnMut() -> T,
    completed: impl Fn(&T) -> bool,
) -> T {
    for _ in 0..ANALYSIS_ATTEMPTS {
        let result = attempt();
        if completed(&result) {
            return result;
        }
    }
    panic!("{label}: language analysis did not complete in {ANALYSIS_ATTEMPTS} attempts");
}

/// `true` when an audit entry in `home` records completed language analysis.
pub fn audit_records_completed_analysis(home: &Path) -> bool {
    fs::read_to_string(home.join(".aegis").join("audit.jsonl")).is_ok_and(|contents| {
        contents.lines().any(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .is_ok_and(|entry| entry["analysis"]["status"] == "complete")
        })
    })
}

pub fn read_audit_entries(home: &Path) -> Vec<serde_json::Value> {
    let path = home.join(".aegis").join("audit.jsonl");
    let contents = fs::read_to_string(path).unwrap();

    contents
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect()
}

/// Write `body` to `path` and make it executable, without this process ever
/// holding an open write handle on `path`.
///
/// With `fs::write`, another test thread can `fork()` while this process holds
/// the write descriptor. The child keeps a copy until its own `exec()` closes
/// it, and running the fake executable inside that window fails with
/// `ETXTBSY` ("Text file busy") (#468). Renaming afterwards does not help: the
/// copied descriptor still points at the same inode. A short-lived `sh` child
/// does the write, so no descriptor of this process refers to the file.
pub fn write_executable(path: &Path, body: &str) {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::process::Stdio;

        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg(r#"cat > "$1" && chmod 755 "$1""#)
            .arg("sh")
            .arg(path)
            .stdin(Stdio::piped())
            .spawn()
            .expect("failed to spawn helper process to write executable");

        child
            .stdin
            .take()
            .expect("child stdin must be piped")
            .write_all(body.as_bytes())
            .expect("failed to write executable contents to helper process");

        let status = child.wait().expect("failed to wait for helper process");
        assert!(status.success(), "failed to write executable at {path:?}");
    }

    #[cfg(not(unix))]
    fs::write(path, body).unwrap();
}

/// Copy the executable at `from` to `to`, keeping its mode, without this
/// process ever holding an open write handle on `to`. `fs::copy` has the same
/// `ETXTBSY` race as `fs::write` in `write_executable` (#659).
pub fn copy_executable(from: &Path, to: &Path) {
    #[cfg(unix)]
    {
        let status = Command::new("/bin/cp")
            .arg("-p")
            .arg(from)
            .arg(to)
            .status()
            .expect("failed to spawn cp to copy executable");
        assert!(status.success(), "failed to copy {from:?} to {to:?}");
    }

    #[cfg(not(unix))]
    fs::copy(from, to).unwrap();
}

pub fn write_disabled_toggle(home: &Path) {
    let aegis_dir = home.join(".aegis");
    fs::create_dir_all(&aegis_dir).unwrap();
    fs::write(aegis_dir.join("disabled"), "timestamp=x\npid=1\n").unwrap();
}

/// Write a global user config (`~/.config/aegis/config.toml`) with the given
/// contents.
///
/// C3 ratchet (ADR-013): the project layer cannot weaken ratcheted fields.
pub fn write_global_config(home: &Path, contents: &str) {
    let global_dir = home.join(".config/aegis");
    fs::create_dir_all(&global_dir).unwrap();
    fs::write(global_dir.join("config.toml"), contents).unwrap();
}

/// Read the invocation log written by a PATH-stub executable.
///
/// Returns the lines recorded by the stub, or an empty `Vec` when the log
/// file does not exist (meaning the stub was never called).
pub fn read_stub_invocations(log_path: &Path) -> Vec<String> {
    match fs::read_to_string(log_path) {
        Ok(contents) => contents
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(str::to_owned)
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn append_snapshot_audit_entry(
    home: &Path,
    command: &str,
    risk: RiskLevel,
    plugin: &str,
    snapshot_id: &str,
) {
    let logger = AuditLogger::new(home.join(".aegis").join("audit.jsonl"));
    logger
        .append(AuditEntry::new(
            command,
            risk,
            Vec::new(),
            Decision::Approved,
            vec![AuditSnapshot {
                plugin: plugin.to_string(),
                snapshot_id: snapshot_id.to_string(),
                git_target: None,
            }],
            None,
            None,
        ))
        .unwrap();
}

pub fn append_audit_entry(home: &Path, plugin: &str, snapshot_id: &str) {
    append_snapshot_audit_entry(
        home,
        "manual test command",
        RiskLevel::Safe,
        plugin,
        snapshot_id,
    );
}

/// Append an approved `Danger` audit entry recording a snapshot for `plugin`.
pub fn seed_snapshot(home: &Path, plugin: &str, snapshot_id: &str) {
    append_snapshot_audit_entry(home, "rm -rf src", RiskLevel::Danger, plugin, snapshot_id);
}

fn aegis_subcommand(home: &Path) -> Command {
    let mut cmd = Command::new(aegis_bin());
    cmd.env("HOME", home).env("AEGIS_CI", "0");
    cmd
}

/// Build an `aegis snapshot list` command configured for `home` with CI mode off.
pub fn snapshot_list_command(home: &Path) -> Command {
    let mut cmd = aegis_subcommand(home);
    cmd.args(["snapshot", "list"]);
    cmd
}

/// Run `aegis snapshot list` for `home` and return its output.
pub fn run_snapshot_list(home: &Path) -> Output {
    snapshot_list_command(home).output().unwrap()
}

/// Run `aegis snapshot prune` with `extra_args` inside `home`.
pub fn run_prune(home: &Path, extra_args: &[&str]) -> Output {
    let mut cmd = aegis_subcommand(home);
    cmd.current_dir(home)
        .args(["snapshot", "prune"])
        .args(extra_args);
    cmd.output().unwrap()
}

/// Empty-command `Safe` assessment used as a neutral baseline in analysis tests.
#[must_use]
pub fn safe_baseline() -> Assessment {
    Assessment {
        risk: RiskLevel::Safe,
        effect_opaque: false,
        matched: Vec::new(),
        highlight_ranges: Vec::new(),
        command: ParsedCommand {
            program: None,
            argv: Vec::new(),
            normalized: String::new(),
            inline_scripts: Vec::new(),
            raw: String::new(),
        },
        analysis: None,
    }
}

/// `true` when `expected` appears as a whole whitespace-delimited token in `output`.
#[must_use]
pub fn version_matches(output: &str, expected: &str) -> bool {
    output.split_whitespace().any(|token| token == expected)
}

/// `true` when this host can confine a command with `config`.
///
/// On a developer host without usable confinement, prints the skip reason and
/// returns `false`. When `AEGIS_CI_REQUIRE_SANDBOX=1` is set (in CI), a
/// missing backend panics so CI never silently skips confinement tests.
pub fn confinement_available_for(config: &aegis_sandbox::SandboxConfig) -> bool {
    let ok = aegis_sandbox::sandbox_available_for(config);
    if !ok {
        assert!(
            std::env::var_os("AEGIS_CI_REQUIRE_SANDBOX").as_deref()
                != Some(std::ffi::OsStr::new("1")),
            "AEGIS_CI_REQUIRE_SANDBOX=1 is set, but no usable confinement backend is available on this host"
        );
        eprintln!("skipping: no usable confinement backend on this host");
    }
    ok
}

/// `true` when this host can confine a command under `/tmp`.
///
/// Delegates to [`confinement_available_for`], printing the skip reason on
/// developer hosts without confinement and panicking when
/// `AEGIS_CI_REQUIRE_SANDBOX=1` is set.
pub fn confinement_available() -> bool {
    let probe = aegis_sandbox::SandboxConfig {
        allow_write: vec![aegis_sandbox::WriteRoot::new("/tmp")],
        trusted_roots: vec![aegis_sandbox::WriteRoot::new("/tmp")],
        ..Default::default()
    };
    confinement_available_for(&probe)
}
