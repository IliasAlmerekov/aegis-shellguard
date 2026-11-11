use super::*;

#[cfg(unix)]
#[test]
fn disabled_watch_without_cwd_reports_error_and_does_not_execute() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let marker = home.path().join("executed");
    write_disabled_toggle(home.path());
    let mut child = Command::new(env!("CARGO_BIN_EXE_aegis"))
        .arg("watch")
        .current_dir(cwd.path())
        .env("HOME", home.path())
        .env("AEGIS_REAL_SHELL", "/bin/sh")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::{BufRead, BufReader, Read};
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    writeln!(
        child.stdin.as_mut().unwrap(),
        "{}",
        serde_json::json!({"cmd": "true", "cwd": home.path(), "id": "ready"})
    )
    .unwrap();
    loop {
        let mut line = String::new();
        if stdout.read_line(&mut line).unwrap() == 0 {
            let mut err = String::new();
            child
                .stderr
                .take()
                .unwrap()
                .read_to_string(&mut err)
                .unwrap();
            panic!("watch exited before ready: {err}");
        }
        let frame: serde_json::Value = serde_json::from_str(&line).unwrap();
        if frame["type"] == "result" && frame["id"] == "ready" {
            assert_eq!(frame["exit_code"], 0);
            break;
        }
    }
    fs::remove_dir(cwd.path()).unwrap();
    let frame = serde_json::json!({"cmd": format!("touch {}", marker.display()), "id": "missing"});
    writeln!(child.stdin.take().unwrap(), "{frame}").unwrap();
    let mut remaining = String::new();
    stdout.read_to_string(&mut remaining).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let frames: Vec<serde_json::Value> = remaining
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["type"], "error");
    assert_eq!(frames[0]["message"], "cwd unavailable");
    assert_eq!(frames[0]["id"], "missing");
    assert!(!marker.exists());
}

#[cfg(unix)]
#[test]
fn enabled_watch_without_cwd_denies_unattended_danger() {
    use std::os::unix::fs::PermissionsExt;
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let marker = home.path().join("executed");
    let stub = home.path().join("terraform");
    fs::write(
        &stub,
        format!("#!/bin/sh\nprintf ran > {}\n", marker.display()),
    )
    .unwrap();
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    let config_dir = home.path().join(".config/aegis");
    fs::create_dir_all(&config_dir).unwrap();
    let user = Command::new("id").arg("-un").output().unwrap();
    let user = String::from_utf8(user.stdout).unwrap();
    fs::write(config_dir.join("config.toml"), format!("allowlist_override_level = \"Danger\"\n[[allow]]\npattern = \"terraform destroy -target=module.test.*\"\nuser = \"{}\"\nreason = \"test\"\n", user.trim())).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_aegis"))
        .arg("watch")
        .current_dir(cwd.path())
        .env("HOME", home.path())
        .env("AEGIS_REAL_SHELL", "/bin/sh")
        .env("AEGIS_CI", "0")
        .env("AEGIS_FORCE_NO_TTY", "1")
        .env(
            "PATH",
            format!(
                "{}:{}",
                home.path().display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::{BufRead, BufReader, Read};
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let stdin = child.stdin.as_mut().unwrap();
    writeln!(
        stdin,
        "{}",
        serde_json::json!({"cmd":"true", "cwd": home.path(), "id":"ready"})
    )
    .unwrap();
    loop {
        let mut line = String::new();
        if stdout.read_line(&mut line).unwrap() == 0 {
            let mut err = String::new();
            child
                .stderr
                .take()
                .unwrap()
                .read_to_string(&mut err)
                .unwrap();
            panic!("{err}");
        }
        let frame: serde_json::Value = serde_json::from_str(&line).unwrap();
        if frame["type"] == "result" && frame["id"] == "ready" {
            break;
        }
    }
    fs::remove_dir(cwd.path()).unwrap();
    writeln!(
        child.stdin.take().unwrap(),
        "{}",
        serde_json::json!({"cmd":"terraform destroy -target=module.test.api", "id":"missing"})
    )
    .unwrap();
    let mut remaining = String::new();
    stdout.read_to_string(&mut remaining).unwrap();
    let output = child.wait_with_output().unwrap();
    let frames = parse_frames(remaining.as_bytes());
    assert!(
        frames
            .iter()
            .any(|frame| frame["type"] == "result" && frame["decision"] == "denied"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!marker.exists());
    let contents = fs::read_to_string(home.path().join(".aegis/audit.jsonl")).unwrap();
    let entry: serde_json::Value = serde_json::from_str(contents.lines().last().unwrap()).unwrap();
    assert_eq!(entry["decision"], "Denied");
    assert_eq!(entry["allowlist_effective"], true);
    assert_eq!(entry["recovery_degradation"], "unavailable_cwd");
}
