use super::ratchet_helpers::{load_global_base, project_ratchet_warnings};
use super::*;

#[test]
fn test_prune_config_defaults_are_disabled() {
    let config = AegisConfig::defaults();

    assert!(!config.prune.enabled, "prune must be disabled by default");
    assert_eq!(
        config.prune.max_count_per_provider, None,
        "default max_count_per_provider must be None"
    );
    assert_eq!(
        config.prune.max_age_days, None,
        "default max_age_days must be None"
    );
}

#[test]
fn test_prune_config_project_layer_cannot_enable_prune_without_global_policy() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();

    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[prune]\nenabled = true\nmax_count_per_provider = 5\nmax_age_days = 30\n",
    )
    .unwrap();

    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();

    assert_eq!(config.prune, PruneConfig::default());
}

#[test]
fn test_prune_config_global_layer_overrides_default() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "[prune]\nenabled = true\nmax_count_per_provider = 3\n",
    )
    .unwrap();

    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();

    assert!(config.prune.enabled);
    assert_eq!(config.prune.max_count_per_provider, Some(3));
}

#[test]
fn test_prune_config_project_layer_can_only_keep_more_snapshots() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "[prune]\nenabled = true\nmax_count_per_provider = 3\nmax_age_days = 7\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[prune]\nenabled = false\nmax_count_per_provider = 10\nmax_age_days = 30\n",
    )
    .unwrap();

    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();

    assert!(!config.prune.enabled);
    assert_eq!(config.prune.max_count_per_provider, Some(10));
    assert_eq!(config.prune.max_age_days, Some(30));
}

#[test]
fn test_prune_config_hostile_project_layer_cannot_widen_deletion() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "[prune]\nenabled = true\nmax_count_per_provider = 3\n",
    )
    .unwrap();
    let project_path = workspace.path().join(PROJECT_CONFIG_FILE);
    fs::write(
        &project_path,
        "[prune]\nenabled = true\nmax_count_per_provider = 0\nmax_age_days = 0\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let layer = ConfigLayerPath {
        source_layer: ConfigSourceLayer::Project,
        path: project_path,
    };
    let warnings = AegisConfig::project_security_ratchet_warnings(&base, &layer).unwrap();

    assert!(config.prune.enabled);
    assert_eq!(config.prune.max_count_per_provider, Some(3));
    assert_eq!(config.prune.max_age_days, None);
    for (field, requested, kept) in [
        ("prune.max_count_per_provider", "0", "3"),
        ("prune.max_age_days", "0", "unset"),
    ] {
        assert!(
            warnings.iter().any(|warning| warning.field == field
                && warning.requested == requested
                && warning.kept == kept),
            "missing ratchet warning for {field}: {warnings:#?}"
        );
    }
}

#[test]
fn test_prune_config_project_layer_cannot_enable_globally_disabled_prune() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "[prune]\nenabled = false\nmax_count_per_provider = 3\n",
    )
    .unwrap();
    let project_path = workspace.path().join(PROJECT_CONFIG_FILE);
    fs::write(&project_path, "[prune]\nenabled = true\n").unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &project_path);

    assert!(!config.prune.enabled);
    assert!(
        warnings.iter().any(|field| field == "prune.enabled"),
        "missing ratchet warning for prune.enabled: {warnings:?}"
    );
}

#[test]
fn test_prune_config_serializes_to_toml() {
    let mut config = AegisConfig::defaults();
    config.prune.enabled = true;
    config.prune.max_count_per_provider = Some(5);
    config.prune.max_age_days = Some(14);

    let toml = config.to_toml_string().unwrap();

    assert!(
        toml.contains("[prune]"),
        "serialized config must include a [prune] section: {toml}"
    );
    assert!(
        toml.contains("max_count_per_provider = 5"),
        "serialized prune count must round-trip: {toml}"
    );
    assert!(
        toml.contains("max_age_days = 14"),
        "serialized prune age must round-trip: {toml}"
    );
}
