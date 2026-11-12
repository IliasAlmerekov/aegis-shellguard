//! Trusted-ceiling warnings as `config validate` reports them (#242).

use std::fs;

use tempfile::TempDir;

use super::{ValidationIssue, validate_config_layers};

const GLOBAL: &str = ".config/aegis/config.toml";

fn run(global: &str) -> (Vec<ValidationIssue>, bool) {
    let cwd = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let path = home.path().join(GLOBAL);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, global).unwrap();
    let report = validate_config_layers(cwd.path(), Some(home.path()));
    (report.warnings, report.valid)
}

fn codes(warnings: &[ValidationIssue]) -> Vec<&'static str> {
    warnings.iter().map(|w| w.code).collect()
}

#[test]
fn enabled_sandbox_outside_a_worktree_warns_workspace_unresolved() {
    let (warnings, valid) = run("[sandbox]\nenabled = true\n");
    assert!(valid, "ceiling warnings never invalidate the config");
    let w = warnings
        .iter()
        .find(|w| w.code == "workspace_unresolved")
        .unwrap_or_else(|| panic!("no workspace_unresolved in {warnings:#?}"));
    assert!(w.message.contains("no_worktree"), "{}", w.message);
}

#[test]
fn disabled_sandbox_does_no_workspace_search_and_raises_no_workspace_warning() {
    let (warnings, valid) = run("[sandbox]\nenabled = false\n");
    assert!(valid);
    assert!(
        !codes(&warnings).contains(&"workspace_unresolved"),
        "{warnings:#?}"
    );
}

#[test]
fn omitted_allow_write_entries_are_reported_with_reason_and_location() {
    let (warnings, valid) =
        run("[sandbox]\nenabled = true\nallow_write = [\"rel/dir\", \"/opt/../etc\"]\n");
    assert!(valid);
    let omitted: Vec<_> = warnings
        .iter()
        .filter(|w| w.code == "trusted_ceiling_path_omitted")
        .collect();
    assert_eq!(omitted.len(), 2, "{warnings:#?}");
    assert!(omitted[0].message.contains("relative") && omitted[0].message.contains("rel/dir"));
    assert!(omitted[1].message.contains("parent_dir"));
    assert!(
        omitted[0].location.ends_with("config.toml"),
        "{}",
        omitted[0].location
    );
    assert!(
        !codes(&warnings).contains(&"workspace_unresolved"),
        "an override list needs no workspace"
    );
}
