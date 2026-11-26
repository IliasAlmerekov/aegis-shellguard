
#[test]
fn project_auto_snapshot_git_cannot_weaken_global_true() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "auto_snapshot_git = true\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "auto_snapshot_git = false\n",
    )
    .unwrap();

    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();

    assert!(
        config.auto_snapshot_git,
        "project auto_snapshot_git must not weaken a globally enabled snapshot flag; got {}",
        config.auto_snapshot_git
    );
}

#[test]
fn project_auto_snapshot_docker_cannot_weaken_global_true() {
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
        "auto_snapshot_docker = false\n",
    )
    .unwrap();

    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();

    assert!(
        config.auto_snapshot_docker,
        "project auto_snapshot_docker must not weaken a globally enabled snapshot flag; got {}",
        config.auto_snapshot_docker
    );
}

#[test]
fn project_auto_snapshot_flags_can_tighten_default_false_to_true() {
    let cases: &[(&str, bool, &str)] = &[
        ("git", true, "auto_snapshot_git"),
        ("docker", false, "auto_snapshot_docker"),
        ("postgres", false, "auto_snapshot_postgres"),
        ("mysql", false, "auto_snapshot_mysql"),
        ("supabase", false, "auto_snapshot_supabase"),
        ("sqlite", false, "auto_snapshot_sqlite"),
    ];

    for (label, default_value, toml_key) in cases {
        let workspace = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();

        fs::write(
            workspace.path().join(PROJECT_CONFIG_FILE),
            format!("{toml_key} = true\n"),
        )
        .unwrap();

        let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();

        let effective = match *label {
            "git" => config.auto_snapshot_git,
            "docker" => config.auto_snapshot_docker,
            "postgres" => config.auto_snapshot_postgres,
            "mysql" => config.auto_snapshot_mysql,
            "supabase" => config.auto_snapshot_supabase,
            "sqlite" => config.auto_snapshot_sqlite,
            _ => unreachable!("unknown auto_snapshot flag {label}"),
        };

        assert!(
            effective,
            "project {toml_key} = true must hold (default {default_value}, {label} tightening); got {effective}"
        );
    }
}

#[test]
fn project_sandbox_allow_network_cannot_weaken_global_false() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "[sandbox]\nallow_network = false\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[sandbox]\nallow_network = true\n",
    )
    .unwrap();

    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();

    assert!(
        !config.sandbox.allow_network,
        "project allow_network must not weaken a globally denied network access; got {}",
        config.sandbox.allow_network
    );
}

#[test]
fn project_sandbox_allow_network_can_tighten_global_true_to_false() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "[sandbox]\nallow_network = true\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[sandbox]\nallow_network = false\n",
    )
    .unwrap();

    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();

    assert!(
        !config.sandbox.allow_network,
        "project must be able to tighten allow_network from global true to false"
    );
}

#[test]
fn project_sandbox_allow_write_cannot_expand_global_base() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "[sandbox]\nallow_write = [\"/opt/data\"]\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[sandbox]\nallow_write = [\"/opt/data\", \"/etc\"]\n",
    )
    .unwrap();

    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();

    assert_eq!(
        allow_write_paths(&config),
        vec![std::path::PathBuf::from("/opt/data")],
        "project allow_write must not expand the writable set beyond the global base; got {:?}",
        allow_write_paths(&config)
    );
}