
#[test]
fn project_docker_scope_names_to_disjoint_names_rejected() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_docker = true\n[docker_scope]\nmode = \"Names\"\nname_patterns = [\"prod-db\"]\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[docker_scope]\nmode = \"Names\"\nname_patterns = [\"x\"]\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.docker_scope.name_patterns,
        vec!["prod-db".to_string()],
        "project must not narrow docker_scope Names patterns to a disjoint set; got {:?}",
        config.docker_scope.name_patterns
    );
    assert_has_warning_for(
        &warnings,
        "docker_scope",
        "bugs-01 Names→disjoint Names rejected",
    );
}

#[test]
fn project_docker_scope_names_subset_patterns_rejected() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_docker = true\n[docker_scope]\nmode = \"Names\"\nname_patterns = [\"a\", \"b\"]\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[docker_scope]\nmode = \"Names\"\nname_patterns = [\"a\"]\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.docker_scope.name_patterns,
        vec!["a".to_string(), "b".to_string()],
        "project must not narrow docker_scope Names patterns to a subset; got {:?}",
        config.docker_scope.name_patterns
    );
    assert_has_warning_for(
        &warnings,
        "docker_scope",
        "bugs-01 Names→subset Names rejected",
    );
}

#[test]
fn project_docker_scope_labeled_to_names_rejected() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_docker = true\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[docker_scope]\nmode = \"Names\"\nname_patterns = [\"x\"]\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.docker_scope.mode,
        crate::snapshot::DockerScopeMode::Labeled,
        "project must not switch docker_scope from Labeled to Names (incomparable); got {:?}",
        config.docker_scope.mode
    );
    assert_eq!(
        config.docker_scope.label, "aegis.snapshot",
        "base Labeled label must be kept when the project attempts an incomparable Names switch; got {:?}",
        config.docker_scope.label
    );
    assert_has_warning_for(
        &warnings,
        "docker_scope",
        "bugs-01 Labeled→Names rejected",
    );
}

#[test]
fn project_docker_scope_labeled_to_labeled_different_label_rejected() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_docker = true\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[docker_scope]\nmode = \"Labeled\"\nlabel = \"other\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.docker_scope.label, "aegis.snapshot",
        "project must not repoint the Labeled docker_scope to a different (incomparable) label; got {:?}",
        config.docker_scope.label
    );
    assert_has_warning_for(
        &warnings,
        "docker_scope",
        "bugs-01 Labeled→Labeled different label rejected",
    );
}

#[test]
fn project_docker_scope_names_to_labeled_rejected() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_docker = true\n[docker_scope]\nmode = \"Names\"\nname_patterns = [\"prod-db\"]\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[docker_scope]\nmode = \"Labeled\"\nlabel = \"aegis.snapshot\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.docker_scope.mode,
        crate::snapshot::DockerScopeMode::Names,
        "project must not switch docker_scope from Names to Labeled (incomparable); got {:?}",
        config.docker_scope.mode
    );
    assert_eq!(
        config.docker_scope.name_patterns,
        vec!["prod-db".to_string()],
        "base Names patterns must be kept when the project attempts an incomparable Labeled switch; got {:?}",
        config.docker_scope.name_patterns
    );
    assert_has_warning_for(
        &warnings,
        "docker_scope",
        "bugs-01 Names→Labeled rejected",
    );
}

#[test]
fn project_docker_scope_names_superset_patterns_allowed() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_docker = true\n[docker_scope]\nmode = \"Names\"\nname_patterns = [\"a\"]\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[docker_scope]\nmode = \"Names\"\nname_patterns = [\"a\", \"b\"]\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.docker_scope.name_patterns,
        vec!["a".to_string(), "b".to_string()],
        "project must be able to broaden docker_scope Names patterns (superset); got {:?}",
        config.docker_scope.name_patterns
    );
    assert_no_warning_for(
        &warnings,
        "docker_scope",
        "bugs-01 Names superset allowed",
    );
}

#[test]
fn project_docker_scope_labeled_to_labeled_same_label_allowed() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_docker = true\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[docker_scope]\nmode = \"Labeled\"\nlabel = \"aegis.snapshot\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.docker_scope.label, "aegis.snapshot",
        "project Labeled with the same label as base must keep the label; got {:?}",
        config.docker_scope.label
    );
    assert_no_warning_for(
        &warnings,
        "docker_scope",
        "bugs-01 Labeled same label allowed",
    );
}


#[test]
fn project_provider_target_not_ratcheted_under_snapshot_policy_none() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "snapshot_policy = \"None\"\nauto_snapshot_postgres = true\n[postgres_snapshot]\ndatabase = \"mydb\"\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[postgres_snapshot]\ndatabase = \"\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.postgres_snapshot.database, "",
        "under SnapshotPolicy::None the postgres target ratchet must not fire; got {:?}",
        config.postgres_snapshot.database
    );
    assert_no_warning_for(
        &warnings,
        "postgres_snapshot",
        "bugs-02 no ratchet under SnapshotPolicy::None",
    );
}


#[test]
fn project_sandbox_allow_write_reordered_subset_no_warning() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "[sandbox]\nenabled = true\nallow_write = [\"/opt\", \"/tmp\"]\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[sandbox]\nallow_write = [\"/tmp\", \"/opt\"]\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    let mut effective = allow_write_paths(&config).clone();
    effective.sort();
    let mut expected = vec![
        std::path::PathBuf::from("/opt"),
        std::path::PathBuf::from("/tmp"),
    ];
    expected.sort();
    assert_eq!(
        effective, expected,
        "reordered equal allow_write set must be preserved as a set; got {:?}",
        allow_write_paths(&config)
    );
    assert_no_warning_for(
        &warnings,
        "sandbox.allow_write",
        "regressions-01 reordered equal subset no warning",
    );
}

#[test]
fn project_allow_write_narrowing_keeps_the_global_list_as_trusted_base() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();
    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "[sandbox]\nallow_write = [\"/opt\", \"/tmp\"]\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[sandbox]\nallow_write = [\"/tmp\"]\n",
    )
    .unwrap();

    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let trusted: Vec<_> = config
        .sandbox
        .allow_write_trusted
        .iter()
        .flatten()
        .map(|root| root.path.clone())
        .collect();
    assert_eq!(
        trusted,
        vec![
            std::path::PathBuf::from("/opt"),
            std::path::PathBuf::from("/tmp")
        ]
    );
}


#[test]
fn project_mysql_snapshot_cannot_repoint_database_when_enabled() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_mysql = true\n[mysql_snapshot]\ndatabase = \"mydb\"\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[mysql_snapshot]\ndatabase = \"otherdb\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.mysql_snapshot.database, "mydb",
        "project must not repoint an enabled mysql target to a decoy database; got {:?}",
        config.mysql_snapshot.database
    );
    assert_has_warning_for(&warnings, "mysql_snapshot.database", "#269 mysql repoint");
}

#[test]
fn project_mysql_snapshot_cannot_repoint_host_or_user_when_enabled() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_mysql = true\n[mysql_snapshot]\ndatabase = \"mydb\"\nhost = \"trusted.internal\"\nuser = \"trusted_user\"\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[mysql_snapshot]\ndatabase = \"mydb\"\nhost = \"decoy.example\"\nuser = \"decoy_user\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(config.mysql_snapshot.host, "trusted.internal");
    assert_eq!(config.mysql_snapshot.user, "trusted_user");
    assert_has_warning_for(&warnings, "mysql_snapshot.host", "#269 mysql host repoint");
    assert_has_warning_for(&warnings, "mysql_snapshot.user", "#269 mysql user repoint");
}

#[test]
fn project_supabase_snapshot_cannot_repoint_db_when_enabled() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_supabase = true\n[supabase_snapshot.db]\ndatabase = \"supadb\"\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[supabase_snapshot.db]\ndatabase = \"otherdb\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.supabase_snapshot.db.database, "supadb",
        "project must not repoint an enabled supabase target to a decoy database; got {:?}",
        config.supabase_snapshot.db.database
    );
    assert_has_warning_for(
        &warnings,
        "supabase_snapshot.db.database",
        "#269 supabase repoint",
    );
}

#[test]
fn project_supabase_snapshot_cannot_repoint_project_ref_or_host_when_enabled() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_supabase = true\n[supabase_snapshot]\nproject_ref = \"trusted_ref\"\n[supabase_snapshot.db]\ndatabase = \"supadb\"\nhost = \"trusted.supabase.co\"\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[supabase_snapshot]\nproject_ref = \"decoy_ref\"\n[supabase_snapshot.db]\nhost = \"decoy.supabase.co\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(config.supabase_snapshot.project_ref, "trusted_ref");
    assert_eq!(config.supabase_snapshot.db.host, "trusted.supabase.co");
    assert_has_warning_for(
        &warnings,
        "supabase_snapshot.project_ref",
        "#269 supabase project_ref repoint",
    );
    assert_has_warning_for(
        &warnings,
        "supabase_snapshot.db.host",
        "#269 supabase host repoint",
    );
}

#[test]
fn project_cannot_disable_supabase_rollback_target_match_when_provider_enabled_in_base() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_supabase = true\n[supabase_snapshot]\nrequire_config_target_match_on_rollback = true\n[supabase_snapshot.db]\ndatabase = \"supadb\"\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[supabase_snapshot]\nrequire_config_target_match_on_rollback = false\nproject_ref = \"decoy_ref\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert!(config.supabase_snapshot.require_config_target_match_on_rollback);
    assert_eq!(config.supabase_snapshot.project_ref, "");
    assert_has_warning_for(
        &warnings,
        "supabase_snapshot.require_config_target_match_on_rollback",
        "#269 rollback target-match cannot be disabled",
    );
    assert_has_warning_for(
        &warnings,
        "supabase_snapshot.project_ref",
        "#269 rollback target-match project_ref repoint",
    );
}

#[test]
fn project_cannot_disable_supabase_rollback_target_match_even_when_provider_not_enabled_in_base() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_supabase = false\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "auto_snapshot_supabase = true\n[supabase_snapshot]\nrequire_config_target_match_on_rollback = false\n[supabase_snapshot.db]\ndatabase = \"projdb\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(config.supabase_snapshot.db.database, "projdb");
    assert!(config.supabase_snapshot.require_config_target_match_on_rollback);
    assert_has_warning_for(
        &warnings,
        "supabase_snapshot.require_config_target_match_on_rollback",
        "#269 rollback target-match unconditional",
    );
}

#[test]
fn global_layer_can_disable_supabase_rollback_target_match_and_repoint_target() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_supabase = true\n\
         [supabase_snapshot]\n\
         require_config_target_match_on_rollback = false\n\
         project_ref = \"global_ref\"\n\
         [supabase_snapshot.db]\n\
         database = \"globaldb\"\n\
         host = \"global.supabase.co\"\n",
    )
    .unwrap();

    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();

    assert!(!config.supabase_snapshot.require_config_target_match_on_rollback);
    assert_eq!(config.supabase_snapshot.project_ref, "global_ref");
    assert_eq!(config.supabase_snapshot.db.database, "globaldb");
    assert_eq!(config.supabase_snapshot.db.host, "global.supabase.co");
}