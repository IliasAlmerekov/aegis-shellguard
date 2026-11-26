#[path = "../../../tests/support/isolated_test.rs"]
mod isolated_test;

use aegis_config::AegisConfig;
use aegis_snapshot::{SnapshotError, SnapshotPlugin, SnapshotRegistry, SnapshotRegistryConfig};
use aegis_types::SnapshotPolicy;

fn in_child(home: bool) -> Option<isolated_test::Completion> {
    const CHILD: &str = "AEGIS_SNAPSHOT_CONTRACT_CHILD";
    if std::env::var_os(CHILD).is_some() {
        return Some(isolated_test::Completion);
    }
    let mut command = isolated_test::command();
    command.env(CHILD, "1");
    command.env_remove("USERPROFILE");
    if home {
        command.env("HOME", std::env::temp_dir());
    } else {
        command.env_remove("HOME");
    }
    isolated_test::run(&mut command);
    None
}

#[test]
fn test_public_api_exports_all_required_types() {
    fn _assert_imports_resolved(
        _plugin: Option<&dyn SnapshotPlugin>,
        _registry: Option<SnapshotRegistry>,
        _config: Option<SnapshotRegistryConfig>,
        _error: Option<SnapshotError>,
    ) {
    }
}

#[test]
fn test_snapshot_error_implements_std_error_send_sync() {
    fn assert_bounds<E: std::error::Error + Send + Sync + 'static>() {}
    assert_bounds::<SnapshotError>();
}

#[test]
fn test_snapshot_registry_config_try_new_succeeds_with_home_set() {
    if let Some(_completion) = in_child(true) {
        let config = AegisConfig::default();
        let result: Result<SnapshotRegistryConfig, SnapshotError> =
            SnapshotRegistryConfig::try_new(&config);
        assert!(result.is_ok(), "try_new failed: {result:?}");
    }
}

#[test]
fn test_snapshot_registry_config_try_new_errors_without_home() {
    if let Some(_completion) = in_child(false) {
        let config = AegisConfig::default();
        let result: Result<SnapshotRegistryConfig, SnapshotError> =
            SnapshotRegistryConfig::try_new(&config);
        assert!(result.is_err(), "expected Err when HOME is unset, got Ok");
    }
}

#[test]
fn test_available_provider_names_contains_all_six_builtins() {
    let names = aegis_snapshot::available_provider_names();
    let expected = ["git", "docker", "postgres", "mysql", "sqlite", "supabase"];

    for name in &expected {
        assert!(
            names.contains(name),
            "available_provider_names() is missing \"{name}\": got {names:?}"
        );
    }
    assert_eq!(
        names.len(),
        expected.len(),
        "available_provider_names() has unexpected entries: {names:?}"
    );
}

#[test]
fn test_snapshot_registry_full_policy_contains_all_six_providers() {
    if let Some(_completion) = in_child(true) {
        let mut config = AegisConfig::default();
        config.snapshot_policy = SnapshotPolicy::Full;

        let registry_config =
            SnapshotRegistryConfig::try_new(&config).expect("try_new failed with HOME set");
        let registry = SnapshotRegistry::from_runtime_config(&registry_config);

        let configured = registry.configured_provider_names();
        let expected = ["git", "docker", "postgres", "mysql", "sqlite", "supabase"];

        for name in &expected {
            assert!(
                configured.contains(name),
                "registry missing provider \"{name}\" under Full policy: got {configured:?}"
            );
        }
        assert_eq!(
            configured.len(),
            expected.len(),
            "registry has unexpected provider count under Full policy: {configured:?}"
        );
    }
}

#[test]
fn test_snapshot_plugin_trait_is_object_safe_and_exported() {
    let _: Option<Box<dyn SnapshotPlugin>> = None;
}
