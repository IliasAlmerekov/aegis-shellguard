use super::*;

#[test]
fn ambiguous_sudo_option_checks_the_executed_candidate() {
    let home = TempDir::new().unwrap();
    let output = base_command(home.path())
        .args([
            "-c",
            "x='-rf /'; sudo -D /tmp/echo rm $x",
            "--output",
            "json",
        ])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_ne!(json["decision"], "auto_approve");
    assert_eq!(json["snapshot_plan"]["requested"], true);
}

#[test]
fn dynamic_printf_option_can_write_shell_state() {
    let home = TempDir::new().unwrap();
    let output = base_command(home.path())
        .args(["-c", "flag=-v; printf $flag HOME /tmp", "--output", "json"])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["decision"], "prompt");
    assert_eq!(json["snapshot_plan"]["requested"], true);
}

#[test]
fn fixed_printf_format_keeps_dynamic_data_read_only() {
    let home = TempDir::new().unwrap();
    let output = base_command(home.path())
        .args(["-c", "printf '%s' \"$DATA\"", "--output", "json"])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["decision"], "auto_approve");
}

#[test]
fn printf_percent_n_with_dynamic_target_requires_approval() {
    for format in ["%n", "%10n", "%-10n", "%+010n", "%ln", "%lln"] {
        let home = TempDir::new().unwrap();
        let command = format!("printf '{format}' \"$target\"");
        let output = base_command(home.path())
            .args(["-c", &command, "--output", "json"])
            .output()
            .unwrap();
        let json: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(json["decision"], "prompt", "{command}");
        assert_eq!(json["snapshot_plan"]["requested"], true, "{command}");
    }
}

#[test]
fn env_split_string_expands_quoted_variables() {
    let home = TempDir::new().unwrap();
    let output = base_command(home.path())
        .args(["-c", "X=-r env -S 'rm ${X} /'", "--output", "json"])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_ne!(json["decision"], "auto_approve");
    if json["decision"] != "block" {
        assert_eq!(json["snapshot_plan"]["requested"], true);
    }
}

#[test]
fn env_split_string_single_dynamic_program_requires_approval() {
    let home = TempDir::new().unwrap();
    let output = base_command(home.path())
        .args(["-c", "CMD=ls env -S '${CMD}'", "--output", "json"])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["decision"], "prompt");
    assert_eq!(json["snapshot_plan"]["requested"], true);
}

#[test]
fn literal_env_split_string_operand_does_not_enable_expansion() {
    for command in [
        "git status env -S '${X}'",
        "/usr/bin/git status env -S '${X}'",
    ] {
        let home = TempDir::new().unwrap();
        let output = base_command(home.path())
            .args(["-c", command, "--output", "json"])
            .output()
            .unwrap();
        let json: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(json["decision"], "auto_approve", "{command}");
    }
}

#[test]
fn dynamic_ripgrep_hostname_program_requires_confirmation_and_recovery() {
    for option in ["--hostname-bin $cmd", "--hostname-bin=$cmd"] {
        let home = TempDir::new().unwrap();
        let command = format!(
            "cmd=/tmp/hostcmd; rg --hyperlink-format 'file://{{host}}{{path}}' {option} needle ."
        );
        let output = base_command(home.path())
            .args(["-c", &command, "--output", "json"])
            .output()
            .unwrap();
        let json: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(json["decision"], "prompt", "{command}");
        assert_eq!(json["snapshot_plan"]["requested"], true, "{command}");
    }
}

#[test]
fn dynamic_interpreter_stdin_requests_recovery() {
    let home = TempDir::new().unwrap();
    let output = base_command(home.path())
        .args(["-c", "cat generated | python3", "--output", "json"])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["decision"], "prompt");
    assert_eq!(json["snapshot_plan"]["requested"], true);
}

#[test]
fn shell_comment_expansion_marker_is_inert() {
    let home = TempDir::new().unwrap();
    let output = base_command(home.path())
        .args(["-c", "git status # inspect $HOME", "--output", "json"])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["decision"], "auto_approve");
}

#[test]
fn heredoc_header_comment_expansion_marker_is_inert() {
    for command in [
        "git status <<EOF # inspect $HOME\nEOF",
        "git status \\\n<<EOF # inspect $HOME\nEOF",
    ] {
        let home = TempDir::new().unwrap();
        let output = base_command(home.path())
            .args(["-c", command, "--output", "json"])
            .output()
            .unwrap();
        let json: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(json["decision"], "auto_approve", "{command}");
    }
}

#[test]
fn env_command_operand_is_not_an_ifs_assignment() {
    let home = TempDir::new().unwrap();
    let output = base_command(home.path())
        .args(["-c", "env echo IFS=,", "--output", "json"])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["decision"], "auto_approve");
}

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
