use super::*;

#[test]
fn dynamic_ripgrep_preprocessor_requires_confirmation_and_recovery() {
    let home = TempDir::new().unwrap();
    let output = base_command(home.path())
        .args([
            "-c",
            "pre=sh; rg --pre \"$pre\" needle .",
            "--output",
            "json",
        ])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["decision"], "prompt");
    assert_eq!(json["snapshot_plan"]["requested"], true);
    assert_eq!(json["execution"]["will_execute"], false);
}

#[test]
fn glued_dynamic_ripgrep_preprocessor_requires_confirmation_and_recovery() {
    let home = TempDir::new().unwrap();
    let output = base_command(home.path())
        .args(["-c", "pre=sh; rg --pre=$pre needle .", "--output", "json"])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["decision"], "prompt");
    assert_eq!(json["snapshot_plan"]["requested"], true);
}

#[test]
fn dynamic_ripgrep_search_pattern_remains_safe() {
    let home = TempDir::new().unwrap();
    let output = base_command(home.path())
        .args([
            "-c",
            "pattern=needle; rg \"$pattern\" .",
            "--output",
            "json",
        ])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["decision"], "auto_approve");
    assert_eq!(json["snapshot_plan"]["requested"], false);
}

#[test]
fn sudo_option_before_env_ifs_assignment_requires_confirmation_and_recovery() {
    let home = TempDir::new().unwrap();
    let output = base_command(home.path())
        .args(["-c", "sudo -u root env IFS=, ls", "--output", "json"])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["decision"], "prompt");
    assert_eq!(json["snapshot_plan"]["requested"], true);
    assert_eq!(json["execution"]["will_execute"], false);
}

#[test]
fn sudo_env_split_string_ifs_assignment_requires_confirmation_and_recovery() {
    let home = TempDir::new().unwrap();
    let output = base_command(home.path())
        .args(["-c", "sudo -u root env -S 'IFS=, ls'", "--output", "json"])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["decision"], "prompt");
    assert_eq!(json["snapshot_plan"]["requested"], true);
}

#[test]
fn sudo_env_glued_split_string_ifs_assignment_requires_confirmation_and_recovery() {
    let home = TempDir::new().unwrap();
    let output = base_command(home.path())
        .args([
            "-c",
            "sudo -u root env --split-string='IFS=, ls'",
            "--output",
            "json",
        ])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["decision"], "prompt");
    assert_eq!(json["snapshot_plan"]["requested"], true);
}

#[test]
fn sudo_env_option_before_split_string_ifs_requires_confirmation_and_recovery() {
    let home = TempDir::new().unwrap();
    let output = base_command(home.path())
        .args([
            "-c",
            "sudo -u root env -i -S 'IFS=, ls'",
            "--output",
            "json",
        ])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["decision"], "prompt");
    assert_eq!(json["snapshot_plan"]["requested"], true);
}

#[test]
fn sudo_env_assignment_before_split_string_ifs_requires_confirmation_and_recovery() {
    let home = TempDir::new().unwrap();
    let output = base_command(home.path())
        .args([
            "-c",
            "sudo -u root env FOO=bar -S 'IFS=, ls'",
            "--output",
            "json",
        ])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["decision"], "prompt");
    assert_eq!(json["snapshot_plan"]["requested"], true);
}
