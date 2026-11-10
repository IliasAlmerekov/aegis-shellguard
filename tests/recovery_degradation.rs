use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde_json::Value;
use tempfile::TempDir;

fn aegis_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_aegis"))
}

fn shell_command(home: &Path, cwd: &Path) -> Command {
    let mut command = Command::new(aegis_bin());
    command
        .env("AEGIS_REAL_SHELL", "/bin/sh")
        .env("AEGIS_CI", "0")
        .env("HOME", home)
        .current_dir(cwd);
    command
}

#[cfg(target_os = "linux")]
fn single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn interactive_shell_output(home: &Path, cwd: &Path, response: &[u8]) -> Output {
    interactive_shell_command_output(
        home,
        cwd,
        "zsh ./run.sh",
        b"to run once without Recovery",
        &[],
        response,
    )
}

/// Runs `shell_line` under a PTY, waits for `prompt`, then sends `response`.
fn interactive_shell_command_output(
    home: &Path,
    cwd: &Path,
    shell_line: &str,
    prompt: &'static [u8],
    envs: &[(&str, &str)],
    response: &[u8],
) -> Output {
    let mut command = Command::new("script");
    command
        .envs(envs.iter().copied())
        .env("AEGIS_REAL_SHELL", "/bin/sh")
        .env("AEGIS_CI", "0")
        .env("HOME", home)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(target_os = "linux")]
    command.args([
        "-qfec",
        &format!(
            "{} -c {}",
            single_quote(&aegis_bin().to_string_lossy()),
            single_quote(shell_line)
        ),
        "/dev/null",
    ]);

    #[cfg(target_os = "macos")]
    command
        .args(["-q", "/dev/null"])
        .arg(aegis_bin())
        .args(["-c", shell_line]);

    let mut child = command.spawn().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (prompt_sender, prompt_receiver) = mpsc::channel();
    let stdout_reader = capture_until_prompt(stdout, prompt, prompt_sender.clone());
    let stderr_reader = capture_until_prompt(stderr, prompt, prompt_sender);

    if prompt_receiver
        .recv_timeout(Duration::from_secs(10))
        .is_err()
    {
        let _ = child.kill();
        let status = child.wait().unwrap();
        let stdout = stdout_reader.join().unwrap();
        let stderr = stderr_reader.join().unwrap();
        panic!(
            "recovery prompt was not observed (status {:?}): stdout=\n{}\nstderr=\n{}",
            status.code(),
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
    }

    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(response).unwrap();

    // BSD `script` writes VEOF directly to the PTY when its stdin closes, while
    // previously read input may still be queued. Keep the pipe open so VEOF
    // cannot overtake the prompt response on macOS.
    #[cfg(target_os = "linux")]
    drop(stdin);

    let status = child.wait().unwrap();
    Output {
        status,
        stdout: stdout_reader.join().unwrap(),
        stderr: stderr_reader.join().unwrap(),
    }
}

fn capture_until_prompt<R>(
    mut reader: R,
    prompt: &'static [u8],
    prompt_sender: mpsc::Sender<()>,
) -> thread::JoinHandle<Vec<u8>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut output = Vec::new();
        let mut buffer = [0_u8; 1024];
        let mut prompt_reported = false;
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(length) => {
                    output.extend_from_slice(&buffer[..length]);
                    if !prompt_reported && output.windows(prompt.len()).any(|part| part == prompt) {
                        prompt_reported = true;
                        let _ = prompt_sender.send(());
                    }
                }
                Err(error) => panic!("failed to read PTY transcript: {error}"),
            }
        }
        output
    })
}

fn read_single_audit_entry(home: &Path) -> Value {
    let contents = fs::read_to_string(home.join(".aegis").join("audit.jsonl")).unwrap();
    serde_json::from_str(contents.trim()).unwrap()
}

fn init_git_repo(path: &Path) {
    let init = Command::new("git")
        .arg("init")
        .current_dir(path)
        .output()
        .unwrap();
    assert!(init.status.success());
    let add = Command::new("git")
        .args(["add", "."])
        .current_dir(path)
        .output()
        .unwrap();
    assert!(add.status.success());
    let commit = Command::new("git")
        .args([
            "-c",
            "user.email=test@aegis.dev",
            "-c",
            "user.name=Aegis Test",
            "commit",
            "-m",
            "init",
        ])
        .current_dir(path)
        .output()
        .unwrap();
    assert!(commit.status.success());
}

#[test]
fn noninteractive_required_recovery_degradation_denies_before_child_execution() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let marker = workspace.path().join("executed");
    fs::write(workspace.path().join("run.sh"), "printf ran > executed\n").unwrap();

    let output = shell_command(home.path(), workspace.path())
        .stdin(Stdio::null())
        .args(["-c", "zsh ./run.sh"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(2),
        "stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!marker.exists(), "degraded command must not execute");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("No required Snapshot was created"));

    let entry = read_single_audit_entry(home.path());
    assert_eq!(entry["decision"], "Denied");
    assert_eq!(entry["effect_opaque"], true);
    assert_eq!(entry["snapshots_required"], true);
    assert_eq!(entry["recovery_degradation"], "no_snapshot_available");
    assert_eq!(entry["snapshots"], serde_json::json!([]));
}

#[test]
fn force_interactive_env_cannot_enable_recovery_override_without_tty() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let marker = workspace.path().join("executed");
    fs::write(workspace.path().join("run.sh"), "printf ran > executed\n").unwrap();

    let mut child = shell_command(home.path(), workspace.path())
        .env("AEGIS_FORCE_INTERACTIVE", "1")
        .args(["-c", "zsh ./run.sh"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.as_mut().unwrap().write_all(b"yes\n").unwrap();
    let output = child.wait_with_output().unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(
        !marker.exists(),
        "a pipe must not enable the recovery override"
    );
    let entry = read_single_audit_entry(home.path());
    assert_eq!(entry["decision"], "Denied");
    assert_eq!(entry["recovery_degradation"], "no_snapshot_available");
}

#[test]
fn interactive_recovery_deny_prevents_child_execution() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let marker = workspace.path().join("executed");
    fs::write(workspace.path().join("run.sh"), "printf ran > executed\n").unwrap();

    let output = interactive_shell_output(home.path(), workspace.path(), b"n\n");

    assert_eq!(output.status.code(), Some(2));
    assert!(!marker.exists());
    let entry = read_single_audit_entry(home.path());
    assert_eq!(entry["decision"], "Denied");
    assert_eq!(entry["recovery_degradation"], "no_snapshot_available");
}

#[test]
fn interactive_recovery_run_once_executes_and_records_human_approval() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let marker = workspace.path().join("executed");
    fs::write(workspace.path().join("run.sh"), "printf ran > executed\n").unwrap();

    let output = interactive_shell_output(home.path(), workspace.path(), b"yes\n");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(marker.exists());
    let entry = read_single_audit_entry(home.path());
    assert_eq!(entry["decision"], "Approved");
    assert_eq!(entry["recovery_degradation"], "no_snapshot_available");
}

#[test]
fn successful_required_snapshot_executes_without_recovery_prompt() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let marker = workspace.path().join("executed");
    fs::write(workspace.path().join("run.sh"), "printf ran > executed\n").unwrap();
    fs::write(workspace.path().join("state.txt"), "before\n").unwrap();
    init_git_repo(workspace.path());
    fs::write(workspace.path().join("state.txt"), "changed\n").unwrap();

    let output = shell_command(home.path(), workspace.path())
        .stdin(Stdio::null())
        .args(["-c", "zsh ./run.sh"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(marker.exists());
    assert!(output.stderr.is_empty(), "no Recovery prompt was expected");
    let entry = read_single_audit_entry(home.path());
    assert_eq!(entry["decision"], "AutoApproved");
    assert!(
        entry["snapshots"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    assert!(entry.get("recovery_degradation").is_none());
}

#[test]
fn auto_approved_run_with_an_unresolved_git_target_halts_before_the_child() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let cwd = workspace.path().canonicalize().unwrap();
    let config_dir = home.path().join(".config").join("aegis");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("config.toml"),
        "allowlist_override_level = \"Danger\"\n",
    )
    .unwrap();
    fs::write(
        cwd.join(".aegis.toml"),
        format!(
            "[[allow]]\npattern = \"git -C /aegis-471-no-such-dir filter-branch*\"\ncwd = \"{}\"\nreason = \"test allowlist\"\n",
            cwd.display()
        ),
    )
    .unwrap();

    let output = shell_command(home.path(), &cwd)
        .stdin(Stdio::null())
        .args([
            "-c",
            "git -C /aegis-471-no-such-dir filter-branch --index-filter true",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(2),
        "stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let entry = read_single_audit_entry(home.path());
    assert_eq!(entry["decision"], "Denied");
    assert_eq!(entry["recovery_degradation"], "unresolved_git_target");
}

#[test]
fn degraded_audit_write_failure_remains_fail_closed() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let marker = workspace.path().join("executed");
    fs::write(home.path().join(".aegis"), "not a directory").unwrap();
    fs::write(workspace.path().join("run.sh"), "printf ran > executed\n").unwrap();

    let output = shell_command(home.path(), workspace.path())
        .stdin(Stdio::null())
        .args(["-c", "zsh ./run.sh"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(4));
    assert!(!marker.exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("failed to write audit log"));
}

#[test]
fn evaluation_reports_required_plan_without_simulating_recovery_degradation() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();

    let output = shell_command(home.path(), workspace.path())
        .args(["-c", "zsh ./run.sh", "--output", "json"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["snapshot_plan"]["requested"], true);
    assert_eq!(
        value["snapshot_plan"]["applicable_plugins"],
        serde_json::json!([])
    );
    assert_eq!(value["execution"]["mode"], "evaluation_only");
    assert_eq!(value["execution"]["will_execute"], false);
    assert!(value.get("recovery_degradation").is_none());
    assert!(!home.path().join(".aegis").join("audit.jsonl").exists());
}

const TERRAFORM_DESTROY: &str = "terraform destroy -target=module.test.api";

/// A workspace whose `terraform` stub drops an `executed` marker, with the
/// Danger command allowlisted so it is auto-approved (ADR-031 section 1).
fn allowlisted_danger_workspace(home: &Path, workspace: &Path) -> (PathBuf, String) {
    let bin = workspace.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let stub = bin.join("terraform");
    fs::write(&stub, "#!/bin/sh\nprintf ran > \"$AEGIS_TEST_MARKER\"\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let config_dir = home.join(".config").join("aegis");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("config.toml"),
        "allowlist_override_level = \"Danger\"\n",
    )
    .unwrap();
    fs::write(
        workspace.join(".aegis.toml"),
        format!(
            "[[allow]]\npattern = \"terraform destroy -target=module.test.*\"\ncwd = \"{}\"\nreason = \"test allowlist\"\n",
            workspace.display()
        ),
    )
    .unwrap();
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    (workspace.join("executed"), path)
}

#[test]
fn allowlisted_danger_without_a_snapshot_provider_halts_without_a_tty() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let cwd = workspace.path().canonicalize().unwrap();
    let (marker, path) = allowlisted_danger_workspace(home.path(), &cwd);

    let output = shell_command(home.path(), &cwd)
        .env("PATH", path)
        .env("AEGIS_TEST_MARKER", &marker)
        .stdin(Stdio::null())
        .args(["-c", TERRAFORM_DESTROY])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(!marker.exists(), "halted Danger must not execute");
    let entry = read_single_audit_entry(home.path());
    assert_eq!(entry["decision"], "Denied");
    assert_eq!(entry["recovery_degradation"], "no_snapshot_available");
}

#[test]
fn allowlisted_danger_with_a_ready_snapshot_runs_without_a_prompt() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let cwd = workspace.path().canonicalize().unwrap();
    let (marker, path) = allowlisted_danger_workspace(home.path(), &cwd);
    fs::write(cwd.join("state.txt"), "before\n").unwrap();
    init_git_repo(&cwd);

    let output = shell_command(home.path(), &cwd)
        .env("PATH", path)
        .env("AEGIS_TEST_MARKER", &marker)
        .stdin(Stdio::null())
        .args(["-c", TERRAFORM_DESTROY])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(marker.exists());
    let entry = read_single_audit_entry(home.path());
    assert_eq!(entry["decision"], "AutoApproved");
    assert!(entry.get("recovery_degradation").is_none());
}

#[test]
fn confirmed_danger_without_a_snapshot_provider_runs_without_a_recovery_prompt() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let cwd = workspace.path().canonicalize().unwrap();
    let (marker, path) = allowlisted_danger_workspace(home.path(), &cwd);
    // Drop the allowlist so a human has to confirm the Danger command.
    fs::remove_file(cwd.join(".aegis.toml")).unwrap();
    let marker_env = marker.to_string_lossy().into_owned();

    let output = interactive_shell_command_output(
        home.path(),
        &cwd,
        TERRAFORM_DESTROY,
        b"[y/N/a/d]",
        &[("PATH", &path), ("AEGIS_TEST_MARKER", &marker_env)],
        b"y\n",
    );

    let transcript = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{transcript}");
    assert!(marker.exists(), "confirmed Danger must execute");
    assert!(!transcript.contains("Type \"yes\""), "{transcript}");
    let entry = read_single_audit_entry(home.path());
    assert_eq!(entry["decision"], "Approved");
    assert!(entry.get("recovery_degradation").is_none());
}

/// `allowlisted_danger_workspace` without the project allowlist, so a human
/// confirms the Danger command. Returns the marker, `PATH`, and the global
/// config path where `ApproveAlways` writes its rule.
fn confirming_danger_workspace(home: &Path, cwd: &Path) -> (PathBuf, String, PathBuf) {
    let (marker, path) = allowlisted_danger_workspace(home, cwd);
    fs::remove_file(cwd.join(".aegis.toml")).unwrap();
    (
        marker,
        path,
        home.join(".config").join("aegis").join("config.toml"),
    )
}

fn approve_always_on_danger(
    home: &Path,
    cwd: &Path,
    marker: &Path,
    path: &str,
    prompt: &'static [u8],
    response: &[u8],
) -> Output {
    let marker_env = marker.to_string_lossy().into_owned();
    interactive_shell_command_output(
        home,
        cwd,
        TERRAFORM_DESTROY,
        prompt,
        &[("PATH", path), ("AEGIS_TEST_MARKER", &marker_env)],
        response,
    )
}

#[test]
fn approve_always_on_danger_with_a_ready_snapshot_writes_the_rule() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let cwd = workspace.path().canonicalize().unwrap();
    let (marker, path, config) = confirming_danger_workspace(home.path(), &cwd);
    fs::write(cwd.join("state.txt"), "before\n").unwrap();
    init_git_repo(&cwd);

    let output = approve_always_on_danger(home.path(), &cwd, &marker, &path, b"[y/N/a/d]", b"a\n");

    assert_eq!(output.status.code(), Some(0));
    assert!(marker.exists());
    assert!(
        fs::read_to_string(config)
            .unwrap()
            .contains("terraform destroy")
    );
}

#[test]
fn approve_always_on_danger_with_a_degraded_snapshot_writes_no_rule_then_shows_the_override() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let cwd = workspace.path().canonicalize().unwrap();
    let (marker, path, config) = confirming_danger_workspace(home.path(), &cwd);

    let output =
        approve_always_on_danger(home.path(), &cwd, &marker, &path, b"[y/N/a/d]", b"a\nn\n");

    let transcript = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(2), "{transcript}");
    assert!(!marker.exists());
    assert!(
        !fs::read_to_string(config)
            .unwrap()
            .contains("terraform destroy")
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let all = format!("{transcript}{stderr}");
    let saved = all.find("allow rule not saved").expect(&all);
    let override_prompt = all.find("to run once without Recovery").expect(&all);
    assert!(saved < override_prompt, "{all}");
}

#[test]
fn approve_always_on_danger_run_once_without_recovery_executes_but_writes_no_rule() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let cwd = workspace.path().canonicalize().unwrap();
    let (marker, path, config) = confirming_danger_workspace(home.path(), &cwd);

    let output =
        approve_always_on_danger(home.path(), &cwd, &marker, &path, b"[y/N/a/d]", b"a\nyes\n");

    assert_eq!(output.status.code(), Some(0));
    assert!(marker.exists());
    assert!(
        !fs::read_to_string(config)
            .unwrap()
            .contains("terraform destroy")
    );
    assert_eq!(
        read_single_audit_entry(home.path())["recovery_degradation"],
        "no_snapshot_available"
    );
}

#[test]
fn approve_always_on_warn_writes_the_rule_with_no_snapshot() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let cwd = workspace.path().canonicalize().unwrap();
    let (_, path, config) = confirming_danger_workspace(home.path(), &cwd);
    let stub = cwd.join("bin").join("git");
    fs::write(&stub, "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let output = interactive_shell_command_output(
        home.path(),
        &cwd,
        "git stash clear",
        b"[y/N/a/d]",
        &[("PATH", &path)],
        b"a\n",
    );

    assert_eq!(output.status.code(), Some(0));
    assert!(fs::read_to_string(config).unwrap().contains("git stash"));
    let entry = read_single_audit_entry(home.path());
    assert_eq!(entry["snapshots"], serde_json::json!([]));
    assert!(entry.get("recovery_degradation").is_none());
}

const PERSISTENT_APPROVAL_UNAVAILABLE: &str =
    "Persistent approval is unavailable while Snapshot recovery is disabled.";

fn snapshot_policy_none_workspace(home: &Path, cwd: &Path) -> (PathBuf, String, PathBuf) {
    let (marker, path, config) = confirming_danger_workspace(home, cwd);
    let mut contents = fs::read_to_string(&config).unwrap();
    contents.push_str("snapshot_policy = \"None\"\n");
    fs::write(&config, contents).unwrap();
    (marker, path, config)
}

#[test]
fn snapshot_policy_none_does_not_offer_always_and_a_one_time_approval_runs_and_is_marked() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let cwd = workspace.path().canonicalize().unwrap();
    let (marker, path, config) = snapshot_policy_none_workspace(home.path(), &cwd);

    let output = approve_always_on_danger(
        home.path(),
        &cwd,
        &marker,
        &path,
        PERSISTENT_APPROVAL_UNAVAILABLE.as_bytes(),
        b"y\n",
    );

    let transcript = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{transcript}");
    assert!(!transcript.contains("[y/N/a/d]"), "{transcript}");
    assert!(marker.exists());
    assert!(
        !fs::read_to_string(config)
            .unwrap()
            .contains("terraform destroy")
    );
    let entry = read_single_audit_entry(home.path());
    assert_eq!(entry["decision"], "Approved");
    assert_eq!(entry["explanation"]["context"]["recovery_opt_out"], true);
    assert!(entry.get("recovery_degradation").is_none());
}

#[test]
fn snapshot_policy_none_rejects_a_typed_always() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let cwd = workspace.path().canonicalize().unwrap();
    let (marker, path, config) = snapshot_policy_none_workspace(home.path(), &cwd);

    let output = approve_always_on_danger(
        home.path(),
        &cwd,
        &marker,
        &path,
        PERSISTENT_APPROVAL_UNAVAILABLE.as_bytes(),
        b"a\n",
    );

    assert_eq!(output.status.code(), Some(2));
    assert!(!marker.exists());
    assert!(
        !fs::read_to_string(config)
            .unwrap()
            .contains("terraform destroy")
    );
}
