//! The surface `aegis-config` promises to callers outside the workspace.
//! A refactor may reshape the internals freely, but removing or narrowing
//! one of these is a breaking change for the crate.

use std::fs;

use aegis_config::model::ConfigLayerPath;
use aegis_config::{AegisConfig, ConfigSourceLayer};
use aegis_types::Mode;
use tempfile::TempDir;

#[test]
fn a_single_layer_can_be_merged_without_runtime_validation() {
    let project = TempDir::new().unwrap();
    let path = project.path().join(".aegis.toml");
    fs::write(&path, "mode = \"Strict\"\n").unwrap();

    let merged = AegisConfig::merge_layer_path_unvalidated(
        AegisConfig::defaults(),
        &ConfigLayerPath {
            source_layer: ConfigSourceLayer::Project,
            path,
        },
    )
    .expect("a well-formed project layer merges cleanly");

    assert_eq!(merged.mode, Mode::Strict);
}

#[test]
fn current_environment_loads_project_config() {
    const CHILD: &str = "AEGIS_CONFIG_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let project = std::env::current_dir().unwrap().join(".aegis.toml");
        assert_eq!(aegis_config::active_config_path_for_append(), Some(project));
        assert_eq!(AegisConfig::load().unwrap().mode, Mode::Strict);
        assert_eq!(AegisConfig::load_inspection().unwrap().mode, Mode::Strict);
        return;
    }

    let home = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    fs::write(project.path().join(".aegis.toml"), "mode = \"Strict\"\n").unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("current_environment_loads_project_config")
        .env(CHILD, "1")
        .env("HOME", home.path())
        .current_dir(project.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}
