//! Trusted-ceiling warnings as `config validate` reports them (#242).

use std::fs;

use tempfile::TempDir;

use super::{ValidationIssue, validate_config_layers};

const GLOBAL: &str = ".config/aegis/config.toml";

fn run(global: &str) -> (Vec<ValidationIssue>, bool) {
    run_layers(global, None)
}

fn run_layers(global: &str, project: Option<&str>) -> (Vec<ValidationIssue>, bool) {
    let cwd = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let path = home.path().join(GLOBAL);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, global).unwrap();
    if let Some(project) = project {
        fs::write(cwd.path().join(".aegis.toml"), project).unwrap();
    }
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

fn with_code<'a>(warnings: &'a [ValidationIssue], code: &str) -> Vec<&'a ValidationIssue> {
    warnings.iter().filter(|w| w.code == code).collect()
}

#[test]
fn project_entry_outside_the_computed_default_raises_the_ratchet_warning() {
    let (warnings, valid) = run_layers(
        "[sandbox]\nenabled = true\n",
        Some("[sandbox]\nallow_write = [\"/etc\"]\n"),
    );
    assert!(valid);
    let ratchet = with_code(&warnings, "project_security_ratchet");
    assert_eq!(ratchet.len(), 1, "{warnings:#?}");
    assert!(ratchet[0].message.contains("sandbox.allow_write"));
    assert!(ratchet[0].location.ends_with(".aegis.toml"), "{}", ratchet[0].location);
    assert!(
        with_code(&warnings, "trusted_ceiling_path_omitted").is_empty(),
        "the lexical case is not double-reported: {warnings:#?}"
    );
}

#[test]
fn project_relative_and_parent_dir_entries_are_omitted_not_clamped() {
    let (warnings, _) = run_layers(
        "[sandbox]\nenabled = true\nallow_write = [\"/tmp\"]\n",
        Some("[sandbox]\nallow_write = [\"rel/dir\", \"/tmp/../etc\"]\n"),
    );
    let omitted = with_code(&warnings, "trusted_ceiling_path_omitted");
    assert_eq!(omitted.len(), 2, "{warnings:#?}");
    assert!(omitted[0].message.contains("relative") && omitted[0].message.contains("project layer"));
    assert!(omitted[1].message.contains("parent_dir") && omitted[1].message.contains("project layer"));
    assert!(omitted.iter().all(|w| w.location.ends_with(".aegis.toml")));
    assert!(
        with_code(&warnings, "project_security_ratchet").is_empty(),
        "{warnings:#?}"
    );
}

#[test]
fn global_invalid_entry_is_reported_even_when_a_project_list_narrows_it() {
    let (warnings, _) = run_layers(
        "[sandbox]\nenabled = true\nallow_write = [\"rel/x\", \"/tmp\"]\n",
        Some("[sandbox]\nallow_write = [\"/tmp\"]\n"),
    );
    let omitted = with_code(&warnings, "trusted_ceiling_path_omitted");
    assert_eq!(omitted.len(), 1, "{warnings:#?}");
    assert!(omitted[0].message.contains("rel/x") && omitted[0].message.contains("global layer"));
}
