
#[test]
fn project_sqlite_snapshot_path_cannot_empty_global_enabled() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_sqlite = true\nsqlite_snapshot_path = \"/opt/db.sqlite\"\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "sqlite_snapshot_path = \"\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.sqlite_snapshot_path, "/opt/db.sqlite",
        "project must not empty a globally-enabled sqlite snapshot path; got {:?}",
        config.sqlite_snapshot_path
    );
    assert_has_warning_for(&warnings, "sqlite_snapshot_path", "C3-01 sqlite empty");
}

#[test]
fn project_sqlite_snapshot_path_cannot_repoint_to_another_nonempty_path() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_sqlite = true\nsqlite_snapshot_path = \"/opt/db.sqlite\"\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "sqlite_snapshot_path = \"/other/db.sqlite\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.sqlite_snapshot_path, "/opt/db.sqlite",
        "project must not repoint an enabled sqlite target to a decoy path; got {:?}",
        config.sqlite_snapshot_path
    );
    assert_has_warning_for(&warnings, "sqlite_snapshot_path", "#269 sqlite repoint");
}

#[test]
fn project_postgres_snapshot_cannot_empty_database_when_enabled() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_postgres = true\n[postgres_snapshot]\ndatabase = \"mydb\"\n",
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
        config.postgres_snapshot.database, "mydb",
        "project must not empty a globally-enabled postgres database; got {:?}",
        config.postgres_snapshot.database
    );
    assert_has_warning_for(&warnings, "postgres_snapshot.database", "C3-01 postgres empty");
}

#[test]
fn project_postgres_snapshot_cannot_repoint_database_when_enabled() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_postgres = true\n[postgres_snapshot]\ndatabase = \"mydb\"\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[postgres_snapshot]\ndatabase = \"otherdb\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.postgres_snapshot.database, "mydb",
        "project must not repoint an enabled postgres target to a decoy database; got {:?}",
        config.postgres_snapshot.database
    );
    assert_has_warning_for(&warnings, "postgres_snapshot.database", "#269 postgres repoint");
}

#[test]
fn project_postgres_snapshot_cannot_repoint_port_when_enabled() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_postgres = true\n[postgres_snapshot]\ndatabase = \"mydb\"\nport = 5432\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[postgres_snapshot]\ndatabase = \"mydb\"\nport = 15432\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(config.postgres_snapshot.port, 5432);
    assert_has_warning_for(&warnings, "postgres_snapshot.port", "#269 postgres port repoint");
}

#[test]
fn project_mysql_snapshot_cannot_empty_database_when_enabled() {
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
        "[mysql_snapshot]\ndatabase = \"\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.mysql_snapshot.database, "mydb",
        "project must not empty a globally-enabled mysql database; got {:?}",
        config.mysql_snapshot.database
    );
    assert_has_warning_for(&warnings, "mysql_snapshot.database", "C3-01 mysql empty");
}

#[test]
fn project_supabase_snapshot_cannot_empty_db_when_enabled() {
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
        "[supabase_snapshot.db]\ndatabase = \"\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.supabase_snapshot.db.database, "supadb",
        "project must not empty a globally-enabled supabase db.database; got {:?}",
        config.supabase_snapshot.db.database
    );
    assert_has_warning_for(&warnings, "supabase_snapshot.db.database", "C3-01 supabase empty");
}

#[test]
fn project_provider_target_not_ratcheted_when_base_disabled() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_postgres = false\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "auto_snapshot_postgres = true\n[postgres_snapshot]\ndatabase = \"projdb\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.postgres_snapshot.database, "projdb",
        "project must be free to configure its own provider when the base leaves it off; got {:?}",
        config.postgres_snapshot.database
    );
    assert_no_warning_for(&warnings, "postgres_snapshot", "C3-01 base-disabled guard");
}

#[test]
fn project_provider_target_ratcheted_under_snapshot_policy_full() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "snapshot_policy = \"Full\"\nsqlite_snapshot_path = \"/opt/db.sqlite\"\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "sqlite_snapshot_path = \"\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.sqlite_snapshot_path, "/opt/db.sqlite",
        "project must not empty a sqlite target enabled via snapshot_policy=Full; got {:?}",
        config.sqlite_snapshot_path
    );
    assert_has_warning_for(
        &warnings,
        "sqlite_snapshot_path",
        "C3-01 Full-policy ratchet",
    );
}

#[test]
fn project_docker_scope_cannot_narrow_all_to_labeled_when_enabled() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_docker = true\n[docker_scope]\nmode = \"All\"\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[docker_scope]\nmode = \"Labeled\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.docker_scope.mode,
        crate::snapshot::DockerScopeMode::All,
        "project must not narrow docker_scope from All to Labeled; got {:?}",
        config.docker_scope.mode
    );
    assert_has_warning_for(&warnings, "docker_scope", "C3-01 docker All→Labeled");
}

#[test]
fn project_docker_scope_cannot_introduce_noop_names_empty_when_enabled() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_docker = true\n[docker_scope]\nmode = \"Labeled\"\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[docker_scope]\nmode = \"Names\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.docker_scope.mode,
        crate::snapshot::DockerScopeMode::Labeled,
        "project must not narrow docker_scope from Labeled to a no-op Names; got {:?}",
        config.docker_scope.mode
    );
    assert_has_warning_for(
        &warnings,
        "docker_scope",
        "C3-01 docker Labeled→Names(empty)",
    );
}

#[test]
fn project_docker_scope_can_broaden_labeled_to_all() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_docker = true\n[docker_scope]\nmode = \"Labeled\"\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[docker_scope]\nmode = \"All\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.docker_scope.mode,
        crate::snapshot::DockerScopeMode::All,
        "project must be able to broaden docker_scope from Labeled to All; got {:?}",
        config.docker_scope.mode
    );
    assert_no_warning_for(&warnings, "docker_scope", "C3-01 docker broaden");
}


#[test]
fn project_sandbox_allow_write_can_tighten_to_subset() {
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
        "[sandbox]\nallow_write = [\"/opt\"]\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        allow_write_paths(&config),
        vec![std::path::PathBuf::from("/opt")],
        "project allow_write tightening to a subset must be honored; got {:?}",
        allow_write_paths(&config)
    );
    assert_no_warning_for(&warnings, "sandbox.allow_write", "C3-02 tighten-subset");
}

#[test]
fn project_sandbox_allow_write_expansion_dropped_and_warned() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "[sandbox]\nenabled = true\nallow_write = [\"/opt\"]\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[sandbox]\nallow_write = [\"/opt\", \"/etc\"]\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        allow_write_paths(&config),
        vec![std::path::PathBuf::from("/opt")],
        "project allow_write expansion must be dropped to the intersection; got {:?}",
        allow_write_paths(&config)
    );
    assert_has_warning_for(&warnings, "sandbox.allow_write", "C3-02 expansion-dropped");
}

#[test]
fn project_sandbox_allow_write_disjoint_tightens_to_empty() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "[sandbox]\nenabled = true\nallow_write = [\"/opt\"]\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[sandbox]\nallow_write = [\"/var\"]\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert!(
        allow_write_paths(&config).is_empty(),
        "project allow_write disjoint from base must tighten to empty; got {:?}",
        allow_write_paths(&config)
    );
    assert_has_warning_for(&warnings, "sandbox.allow_write", "C3-02 disjoint-empty");
}

#[test]
fn project_sandbox_allow_write_narrower_trusted_root_survives_wider_project_root() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();
    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "[sandbox]\nallow_write = [\"/workspace/src\"]\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[sandbox]\nallow_write = [\"/workspace\"]\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        allow_write_paths(&config),
        vec![std::path::PathBuf::from("/workspace/src")]
    );
    assert_eq!(warnings, vec!["sandbox.allow_write".to_string()]);
    let root = &config.sandbox.allow_write.as_ref().unwrap()[0];
    assert_eq!(root.layer, aegis_types::CeilingLayer::Global);
}

#[test]
fn project_sandbox_allow_write_narrows_to_deeper_project_root_without_warning() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();
    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "[sandbox]\nallow_write = [\"/workspace\"]\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[sandbox]\nallow_write = [\"/workspace/src\"]\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        allow_write_paths(&config),
        vec![std::path::PathBuf::from("/workspace/src")]
    );
    assert!(warnings.is_empty(), "tightening must not warn: {warnings:?}");
    let root = &config.sandbox.allow_write.as_ref().unwrap()[0];
    assert_eq!(root.layer, aegis_types::CeilingLayer::Project);
}

#[test]
fn project_sandbox_allow_write_parent_dir_entry_is_kept_for_omission_not_folded() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();
    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "[sandbox]\nallow_write = [\"/opt\"]\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[sandbox]\nallow_write = [\"/opt/../etc\"]\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        allow_write_paths(&config),
        vec![std::path::PathBuf::from("/opt/../etc")]
    );
    assert_no_warning_for(&warnings, "sandbox.allow_write", "parent_dir kept");
}


#[test]
fn partial_sandbox_rejects_unknown_field_require() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();

    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[sandbox]\nrequire = true\n",
    )
    .unwrap();

    let result = AegisConfig::load_for(workspace.path(), Some(home.path()));
    assert!(
        result.is_err(),
        "misspelled `[sandbox] require = true` must be rejected, not silently ignored; got Ok({:?})",
        result.ok()
    );
}

#[test]
fn partial_sandbox_rejects_unknown_field_allow_netork() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();

    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[sandbox]\nallow_netork = false\n",
    )
    .unwrap();

    let result = AegisConfig::load_for(workspace.path(), Some(home.path()));
    assert!(
        result.is_err(),
        "misspelled `[sandbox] allow_netork = false` must be rejected, not silently ignored; got Ok({:?})",
        result.ok()
    );
}

#[test]
fn direct_sandbox_rejects_unknown_field_require() {
    let toml_src = "[sandbox]\nrequire = true\n";
    let result = toml::from_str::<AegisConfig>(toml_src);
    assert!(
        result.is_err(),
        "misspelled `[sandbox] require = true` must be rejected on the direct AegisConfig path; got Ok({:?})",
        result.ok()
    );
}