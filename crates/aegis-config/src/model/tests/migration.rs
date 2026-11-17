use super::*;

#[test]
fn malformed_project_config_is_fatal() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "mode = \"Strict\"\nauto_snapshot_git = false\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "mode = <<<THIS IS NOT VALID TOML\n",
    )
    .unwrap();

    let err = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap_err();
    let message = err.to_string();

    assert!(
        message.contains(
            &workspace
                .path()
                .join(PROJECT_CONFIG_FILE)
                .display()
                .to_string()
        ),
        "error must identify the malformed project config file: {message}"
    );
    assert!(
        message.contains("failed to parse"),
        "error must preserve the parse failure details: {message}"
    );
}

// ── Snapshot policy tests ───────────────────────────────────────

#[test]
fn snapshot_policy_defaults_to_selective() {
    let config = AegisConfig::defaults();
    assert_eq!(config.snapshot_policy, SnapshotPolicy::Selective);
}

#[test]
fn snapshot_policy_none_deserializes() {
    let config: AegisConfig = toml::from_str(r#"snapshot_policy = "None""#).unwrap();
    assert_eq!(config.snapshot_policy, SnapshotPolicy::None);
}

#[test]
fn snapshot_policy_selective_deserializes() {
    let config: AegisConfig = toml::from_str(r#"snapshot_policy = "Selective""#).unwrap();
    assert_eq!(config.snapshot_policy, SnapshotPolicy::Selective);
}

#[test]
fn snapshot_policy_full_deserializes() {
    let config: AegisConfig = toml::from_str(r#"snapshot_policy = "Full""#).unwrap();
    assert_eq!(config.snapshot_policy, SnapshotPolicy::Full);
}

#[test]
fn snapshot_policy_none_ignores_per_plugin_flags() {
    let config: AegisConfig = toml::from_str(
        r#"
snapshot_policy = "None"
auto_snapshot_git = true
auto_snapshot_docker = true
"#,
    )
    .unwrap();
    assert_eq!(config.snapshot_policy, SnapshotPolicy::None);
}

#[test]
fn snapshot_policy_full_enables_all_regardless_of_flags() {
    let config: AegisConfig = toml::from_str(
        r#"
snapshot_policy = "Full"
auto_snapshot_git = false
auto_snapshot_docker = false
"#,
    )
    .unwrap();
    assert_eq!(config.snapshot_policy, SnapshotPolicy::Full);
}

#[test]
fn project_snapshot_policy_cannot_weaken_global_full_to_none() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);

    fs::create_dir_all(&global_dir).unwrap();
    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "snapshot_policy = \"Full\"\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "snapshot_policy = \"None\"\n",
    )
    .unwrap();

    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();

    assert_eq!(config.snapshot_policy, SnapshotPolicy::Full);
}

#[test]
fn project_cannot_disable_effect_opaque_recovery_over_default_selective() {
    // ADR-016 / H9: recovery is the primary v1 backstop for effect-opaque
    // execution, and it keys off `snapshot_policy != None`. The C3 ratchet must
    // keep that requirement intact: a project `.aegis.toml` cannot disable
    // recovery by lowering `snapshot_policy` to `None`. Over the default
    // `Selective` base, a project `None` is rejected and the base is kept — so
    // an effect-opaque command still earns a pre-exec snapshot.
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();

    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "snapshot_policy = \"None\"\n",
    )
    .unwrap();

    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();

    assert_eq!(
        config.snapshot_policy,
        SnapshotPolicy::Selective,
        "project must not weaken the default recovery posture to None"
    );
}

#[test]
fn project_snapshot_policy_can_tighten_default_selective_to_full() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();

    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "snapshot_policy = \"Full\"\n",
    )
    .unwrap();

    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();

    assert_eq!(config.snapshot_policy, SnapshotPolicy::Full);
}

#[test]
fn snapshot_policy_absent_in_overlay_keeps_base() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);

    fs::create_dir_all(&global_dir).unwrap();
    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "snapshot_policy = \"None\"\n",
    )
    .unwrap();
    // Project sets only mode, not snapshot_policy.
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "mode = \"Audit\"\n",
    )
    .unwrap();

    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    assert_eq!(config.snapshot_policy, SnapshotPolicy::None);
}

#[test]
fn user_pattern_deserializes_justification() {
    let pattern: UserPattern = toml::from_str(
        r#"
id = "USR-001"
category = "Cloud"
risk = "Warn"
pattern = "test"
description = "desc"
justification = "because"
"#,
    )
    .unwrap();
    assert_eq!(pattern.justification, Some("because".to_string()));
}

#[test]
fn user_pattern_justification_is_optional() {
    let pattern: UserPattern = toml::from_str(
        r#"
id = "USR-002"
category = "Cloud"
risk = "Warn"
pattern = "test"
description = "desc"
"#,
    )
    .unwrap();
    assert_eq!(pattern.justification, None);
}

#[test]
fn custom_pattern_with_justification_roundtrips_through_config() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();

    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        r#"
[[custom_patterns]]
id = "USR-JST"
category = "Cloud"
risk = "Danger"
pattern = "terraform destroy"
description = "Terraform destroy guard"
justification = "This tears down all provisioned infrastructure. Confirm you are in the correct workspace and have state backups."
safe_alt = "terraform plan -destroy"
"#,
    )
    .unwrap();

    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    assert_eq!(config.custom_patterns.len(), 1);
    assert_eq!(config.custom_patterns[0].id, "USR-JST");
    assert_eq!(
        config.custom_patterns[0].justification,
        Some("This tears down all provisioned infrastructure. Confirm you are in the correct workspace and have state backups.".to_string())
    );
}

#[test]
fn legacy_blocklist_table_name_deserializes_into_block_field() {
    let config: AegisConfig = toml::from_str(
        r#"
[[blocklist]]
pattern = "rm -rf /"
cwd = "/tmp"
reason = "never delete root"
"#,
    )
    .unwrap();

    assert_eq!(config.blocklist.len(), 1);
    assert_eq!(config.blocklist[0].pattern, "rm -rf /");
    assert_eq!(config.blocklist[0].cwd, Some("/tmp".to_string()));
    assert_eq!(config.blocklist[0].reason, "never delete root");
}

#[test]
fn block_rule_deserializes_from_toml() {
    let parsed: AegisConfig = toml::from_str(
        r#"[[block]]
pattern = "x"
cwd = "/"
reason = "test"
"#,
    )
    .unwrap();
    assert_eq!(parsed.blocklist.len(), 1);
}
