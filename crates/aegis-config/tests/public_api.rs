//! The surface `aegis-config` promises to callers outside the workspace.
//! A refactor may reshape the internals freely, but removing or narrowing
//! one of these is a breaking change for the crate.

use std::fs;

use aegis_config::model::ConfigLayerPath;
use aegis_config::{AegisConfig, ConfigSourceLayer, Mode};
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
