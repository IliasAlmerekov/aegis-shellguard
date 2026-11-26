use std::path::Path;

use tempfile::TempDir;

use super::AegisConfig;
use super::ConfigLayerPath;
use super::ConfigSourceLayer;

pub(super) fn load_global_base(home: &Path) -> AegisConfig {
    let scratch = TempDir::new().unwrap();
    AegisConfig::load_for_inspection(scratch.path(), Some(home)).unwrap()
}

pub(super) fn project_ratchet_warnings(base: &AegisConfig, project_path: &Path) -> Vec<String> {
    let layer = ConfigLayerPath {
        source_layer: ConfigSourceLayer::Project,
        path: project_path.to_path_buf(),
    };
    AegisConfig::project_security_ratchet_warnings(base, &layer)
        .map(|warnings| warnings.into_iter().map(|w| w.field.to_string()).collect())
        .unwrap_or_default()
}

pub(super) fn assert_no_warning_for(fields: &[String], expected: &str, msg: &str) {
    assert!(
        !fields.iter().any(|f| f == expected),
        "{msg}: expected NO `{expected}` warning but got {fields:?}"
    );
}

pub(super) fn assert_has_warning_for(fields: &[String], expected: &str, msg: &str) {
    assert!(
        fields.iter().any(|f| f == expected),
        "{msg}: expected a `{expected}` warning but got {fields:?}"
    );
}

pub(super) fn project_ratchet_warning_strings(
    base: &AegisConfig,
    project_path: &Path,
    field: &str,
) -> Option<(String, String)> {
    let layer = ConfigLayerPath {
        source_layer: ConfigSourceLayer::Project,
        path: project_path.to_path_buf(),
    };
    AegisConfig::project_security_ratchet_warnings(base, &layer)
        .ok()?
        .into_iter()
        .find(|w| w.field == field)
        .map(|w| (w.requested, w.kept))
}

pub(super) fn allow_write_paths(config: &AegisConfig) -> Vec<std::path::PathBuf> {
    config
        .sandbox
        .allow_write
        .iter()
        .flatten()
        .map(|root| root.path.clone())
        .collect()
}
