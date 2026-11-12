use std::fs;

use super::{GLOBAL_CONFIG_DIR, GLOBAL_CONFIG_FILE, validate_config_layers};
use tempfile::TempDir;

fn write_policy_star(home: &std::path::Path) -> std::path::PathBuf {
    let dir = home.join(".aegis");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("policy.star");
    fs::write(&path, "").unwrap();
    path
}

fn unsupported_policy_star_message(path: &std::path::Path) -> String {
    format!(
        "policy.star at {} is no longer supported: Aegis reads Policy rules only from [[rules]] in aegis.toml. Move the rules there and delete the file.",
        path.display()
    )
}

#[test]
fn reject_policy_star_fails_with_the_migration_message_when_the_file_exists() {
    let home = TempDir::new().unwrap();
    let path = write_policy_star(home.path());

    let err = super::reject_policy_star(Some(home.path())).unwrap_err();

    assert_eq!(
        err.to_string(),
        format!("config error: {}", unsupported_policy_star_message(&path))
    );
}

#[test]
fn reject_policy_star_passes_without_the_file_or_a_home() {
    let home = TempDir::new().unwrap();

    assert!(super::reject_policy_star(Some(home.path())).is_ok());
    assert!(super::reject_policy_star(None).is_ok());
}

#[test]
fn validate_config_layers_reports_policy_star_when_no_config_layer_exists() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let path = write_policy_star(home.path());

    let report = validate_config_layers(workspace.path(), Some(home.path()));

    assert!(!report.valid);
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.errors[0].code, "unsupported_policy_star");
    assert_eq!(report.errors[0].location, path.to_string_lossy());
    assert_eq!(
        report.errors[0].message,
        format!("config error: {}", unsupported_policy_star_message(&path))
    );
}

#[test]
fn validate_config_layers_reports_policy_star_and_a_broken_layer_together() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    write_policy_star(home.path());
    fs::write(workspace.path().join(".aegis.toml"), "not valid toml [[[").unwrap();

    let report = validate_config_layers(workspace.path(), Some(home.path()));

    let codes: Vec<_> = report.errors.iter().map(|e| e.code).collect();
    assert!(!report.valid);
    assert_eq!(codes, ["unsupported_policy_star", "config_parse_error"]);
}

#[test]
fn validate_config_layers_keeps_policy_star_when_a_layer_checkpoint_fails() {
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    write_policy_star(home.path());
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();
    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "[audit]\nrotation_enabled = true\nmax_file_size_bytes = 0\nretention_files = 1\n",
    )
    .unwrap();

    let report = validate_config_layers(workspace.path(), Some(home.path()));

    assert!(!report.valid);
    assert_eq!(report.errors[0].code, "unsupported_policy_star");
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.code == "audit_max_file_size")
    );
}
