#![expect(
    clippy::print_stderr,
    dead_code,
    reason = "shared by many test crates that each use a subset"
)]

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

pub fn git(repo: &Path, args: &[&str]) -> String {
    String::from_utf8_lossy(&run_git(repo, args).stdout)
        .trim()
        .to_string()
}

pub fn stash_hashes(repo: &Path) -> Vec<String> {
    git(repo, &["stash", "list", "--format=%H"])
        .lines()
        .map(str::to_owned)
        .collect()
}

pub fn init_git_repo(repo: &Path) {
    run_git(repo, &["init", "-q"]);
    run_git(repo, &["config", "user.email", "test@aegis.dev"]);
    run_git(repo, &["config", "user.name", "Aegis Test"]);
    run_git(repo, &["config", "core.autocrlf", "false"]);
    run_git(repo, &["add", "-A"]);
    run_git(repo, &["commit", "-q", "--allow-empty", "-m", "init"]);
}

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

#[must_use]
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[must_use]
pub fn repo_path(relative: &str) -> PathBuf {
    repo_root().join(relative)
}

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
    command.env("AEGIS_CI", "0");
    command.env("HOME", home);
    command
}

pub fn direct_shell_command(home: &Path) -> Command {
    let mut command = Command::new("/bin/sh");
    command.env("HOME", home);
    command
}

pub const ANALYSIS_ATTEMPTS: usize = 5;

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

pub fn write_global_config(home: &Path, contents: &str) {
    let global_dir = home.join(".config/aegis");
    fs::create_dir_all(&global_dir).unwrap();
    fs::write(global_dir.join("config.toml"), contents).unwrap();
}

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

pub fn seed_snapshot(home: &Path, plugin: &str, snapshot_id: &str) {
    append_snapshot_audit_entry(home, "rm -rf src", RiskLevel::Danger, plugin, snapshot_id);
}

fn aegis_subcommand(home: &Path) -> Command {
    let mut cmd = Command::new(aegis_bin());
    cmd.env("HOME", home).env("AEGIS_CI", "0");
    cmd
}

pub fn snapshot_list_command(home: &Path) -> Command {
    let mut cmd = aegis_subcommand(home);
    cmd.args(["snapshot", "list"]);
    cmd
}

pub fn run_snapshot_list(home: &Path) -> Output {
    snapshot_list_command(home).output().unwrap()
}

pub fn run_prune(home: &Path, extra_args: &[&str]) -> Output {
    let mut cmd = aegis_subcommand(home);
    cmd.current_dir(home)
        .args(["snapshot", "prune"])
        .args(extra_args);
    cmd.output().unwrap()
}

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

#[must_use]
pub fn version_matches(output: &str, expected: &str) -> bool {
    output.split_whitespace().any(|token| token == expected)
}

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

pub fn confinement_available() -> bool {
    let probe = aegis_sandbox::SandboxConfig {
        allow_write: vec![aegis_sandbox::WriteRoot::new("/tmp")],
        trusted_roots: vec![aegis_sandbox::WriteRoot::new("/tmp")],
        ..Default::default()
    };
    confinement_available_for(&probe)
}

pub fn workspace_msrv() -> String {
    let manifest =
        fs::read_to_string(repo_path("Cargo.toml")).expect("Cargo.toml should be readable");

    manifest
        .lines()
        .map(str::trim)
        .skip_while(|line| *line != "[workspace.package]")
        .skip(1)
        .take_while(|line| !line.starts_with('['))
        .find_map(|line| line.strip_prefix("rust-version = "))
        .map(|value| value.trim_matches('"').to_string())
        .expect("Cargo.toml should set rust-version in [workspace.package]")
}
