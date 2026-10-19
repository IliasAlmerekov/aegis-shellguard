//! #525: an unattended auto-approved run halts when a `cd` earlier in the
//! command line leaves the Snapshot location unknown, and does not halt when
//! the directory is a plain one a non-git stage can run in.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;
use tempfile::TempDir;

fn toml_string(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    )
}

struct World {
    home: TempDir,
    _root: TempDir,
    cwd: PathBuf,
}

fn commit_repo(dir: &Path) {
    fs::write(dir.join("f"), "x\n").unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["add", "."],
        vec![
            "-c",
            "user.email=t@aegis.dev",
            "-c",
            "user.name=T",
            "commit",
            "-q",
            "-m",
            "init",
        ],
    ] {
        let out = Command::new("git")
            .args(&args)
            .current_dir(dir)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .output()
            .unwrap();
        assert!(out.status.success());
    }
}

/// A cwd that is a git repository with one commit, next to a plain directory
/// and a second repository, `other`.
fn world(mode: &str) -> World {
    let home = TempDir::new().unwrap();
    let root = TempDir::new().unwrap();
    let base = root.path().canonicalize().unwrap();
    let cwd = base.join("cwd");
    fs::create_dir_all(&cwd).unwrap();
    fs::create_dir_all(base.join("plain")).unwrap();
    commit_repo(&cwd);
    let other = base.join("other");
    fs::create_dir_all(&other).unwrap();
    commit_repo(&other);
    let config_dir = home.path().join(".config").join("aegis");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("config.toml"),
        format!("mode = \"{mode}\"\nallowlist_override_level = \"Danger\"\n"),
    )
    .unwrap();
    World {
        home,
        _root: root,
        cwd,
    }
}

fn run_allowlisted(world: &World, command: &str, cdpath: Option<&str>) -> Output {
    fs::write(
        world.cwd.join(".aegis.toml"),
        format!(
            "[[allow]]\npattern = {}\ncwd = {}\nreason = \"test allowlist\"\n",
            toml_string(command),
            toml_string(&world.cwd.display().to_string())
        ),
    )
    .unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_aegis"));
    cmd.env("AEGIS_REAL_SHELL", "/bin/sh")
        .env("AEGIS_CI", "0")
        .env("HOME", world.home.path())
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("CDPATH")
        .current_dir(&world.cwd)
        .stdin(Stdio::null());
    if let Some(cdpath) = cdpath {
        cmd.env("CDPATH", cdpath);
    }
    cmd.args(["-c", command]).output().unwrap()
}

fn audit_entry(home: &Path) -> Value {
    let contents = fs::read_to_string(home.join(".aegis").join("audit.jsonl")).unwrap();
    serde_json::from_str(contents.lines().last().unwrap()).unwrap()
}

fn assert_halted(world: &World, command: &str, cdpath: Option<&str>) -> Value {
    let output = run_allowlisted(world, command, cdpath);
    assert_eq!(
        output.status.code(),
        Some(2),
        "{command:?}\nstdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let entry = audit_entry(world.home.path());
    assert_eq!(entry["decision"], "Denied", "{command:?}");
    assert_eq!(
        entry["recovery_degradation"], "unresolved_git_target",
        "{command:?}"
    );
    entry
}

#[test]
fn unresolved_cd_forms_halt_an_unattended_run_before_the_child() {
    let world = world("Protect");
    for command in [
        "cd && rm -rf y",
        "cd - && rm -rf y",
        "cd -P ../plain && rm -rf y",
        "pushd ../plain && rm -rf y",
        "cd ~/x && rm -rf y",
        "cd ../pl* && rm -rf y",
        "cd ../plain; rm -rf y",
        "cd ../plain || rm -rf y",
        "cd ../plain\nrm -rf y",
        "mkdir n && cd n && rm -rf x",
        "cd ../gone && rm -rf x",
    ] {
        assert_halted(&world, command, None);
    }
}

#[test]
fn a_variable_cd_path_is_disclosed_and_never_runs_unattended() {
    // A `$` path makes the command effect-opaque, so the allowlist cannot
    // auto-approve it (ADR-043). The run is denied before the child, and the
    // audit entry still names the unresolved target.
    let world = world("Protect");

    let output = run_allowlisted(&world, "cd \"$D\" && rm -rf y", None);

    assert_eq!(output.status.code(), Some(2));
    let entry = audit_entry(world.home.path());
    assert_eq!(entry["decision"], "Denied");
    assert_eq!(
        entry["explanation"]["context"]["git_targets"][0]["status"],
        "unresolved"
    );
}

#[test]
fn relative_cd_with_cdpath_set_halts_an_unattended_run() {
    let world = world("Protect");

    assert_halted(&world, "cd plain && rm -rf y", Some("/tmp"));
}

#[test]
fn dot_relative_cd_with_cdpath_set_does_not_halt() {
    let world = world("Protect");

    let output = run_allowlisted(&world, "cd ../plain && rm -rf y", Some("/tmp"));

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn the_audit_entry_names_why_the_cd_target_is_unresolved() {
    let world = world("Protect");

    let entry = assert_halted(&world, "cd ../plain; rm -rf y", None);

    let targets = entry["explanation"]["context"]["git_targets"].to_string();
    assert!(
        targets.contains("may not have run"),
        "audit entry must name the reason: {targets}"
    );
}

#[test]
fn a_plain_directory_does_not_halt_a_non_git_stage() {
    let world = world("Protect");

    let output = run_allowlisted(&world, "cd ../plain && rm -rf y", None);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(audit_entry(world.home.path())["recovery_degradation"].is_null());
}

#[test]
fn a_git_stage_in_a_plain_directory_halts() {
    let world = world("Protect");

    assert_halted(
        &world,
        "cd ../plain && git filter-branch --index-filter true",
        None,
    );
}

#[test]
fn audit_mode_does_not_halt_for_an_unresolved_cd() {
    let world = world("Audit");

    let output = run_allowlisted(&world, "cd ../plain; rm -rf y", None);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let entry = audit_entry(world.home.path());
    assert!(entry["recovery_degradation"].is_null());
    assert!(entry["snapshots"].as_array().is_none_or(Vec::is_empty));
}

/// The snapshot ids a run recorded, by the directory they name.
fn snapshot_ids(entry: &Value) -> Vec<String> {
    entry["snapshots"]
        .as_array()
        .map(|snaps| {
            snaps
                .iter()
                .map(|snap| snap["snapshot_id"].as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_cd_in_a_pipeline_stage_leaves_the_process_cwd_snapshot() {
    let world = world("Protect");

    let output = run_allowlisted(&world, "cd ../other | rm -rf y", None);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr=\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let entry = audit_entry(world.home.path());
    assert!(entry["recovery_degradation"].is_null(), "{entry}");
    let ids = snapshot_ids(&entry);
    assert_eq!(ids.len(), 1, "{entry}");
    assert!(
        ids[0].starts_with(&world.cwd.display().to_string()),
        "the Snapshot must be of the process cwd, not `other`: {ids:?}"
    );
}

#[test]
fn a_subshell_group_is_denied_for_analysis_not_for_an_unresolved_target() {
    // A `( ... )` group is effect-opaque (ADR-043), so an allowlist cannot
    // approve it and the run is denied before any Snapshot. The denial must
    // come from that analysis rule: the `cd` inside or before the group is
    // resolved, not disclosed as an unresolved Git target. The targets a
    // group plans (`x` for `(cd x && D)`, the process cwd for `(cd x); D`)
    // are pinned by the planning tests in `git_target_plan_tests.rs`.
    let world = world("Protect");
    for command in ["(cd ../other); rm -rf y", "(cd ../other && rm -rf y)"] {
        let output = run_allowlisted(&world, command, None);

        assert_eq!(output.status.code(), Some(2), "{command:?}");
        let entry = audit_entry(world.home.path());
        assert_eq!(entry["decision"], "Denied", "{command:?}");
        assert!(
            entry["recovery_degradation"].is_null(),
            "{command:?}: {entry}"
        );
        assert_eq!(entry["effect_opaque"], true, "{command:?}");
    }
}
