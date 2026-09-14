// ── bugs-01: docker ratchet must reject intra-rank narrowing/incomparable ─
// The current `ratchet_docker_scope` is rank-only: same-rank moves (Names→
// different Names, Labeled→Names, Labeled→different label, Names→Labeled) are
// KEPT (project wins) with no warning. Desired: under the Project layer, when
// the docker provider is ENABLED in the trusted base AND the base scope is not
// a no-op, a project overlay that NARROWS or is INCOMPARABLE with the base
// eligible-container set is rejected (keep base + warn). Only keep-or-broaden
// is permitted.

#[test]
fn project_docker_scope_names_to_disjoint_names_rejected() {
    // bugs-01 RED: base Names `["prod-db"]`; project Names `["x"]` (disjoint,
    // same rank). Current code keeps project `["x"]` with no warning. Desired:
    // keep base `["prod-db"]` + warn — overlay patterns are NOT a superset of
    // base patterns (some base pattern absent from overlay).
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
    // bugs-01 RED: base Names `["a", "b"]`; project Names `["a"]` (subset, same
    // rank). Current code keeps project `["a"]` with no warning. Desired: keep
    // base `["a", "b"]` + warn — overlay patterns are NOT a superset of base
    // patterns (base "b" absent from overlay).
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
    // bugs-01 RED: base default `Labeled` (label "aegis.snapshot"); project
    // `Names` with `["x"]` (same rank, incomparable mode switch). Current code
    // keeps project `Names` with no warning. Desired: keep base `Labeled` +
    // label "aegis.snapshot" + warn.
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
    // bugs-01 RED: base default `Labeled` (label "aegis.snapshot"); project
    // `Labeled` with a different label "other" (same rank, incomparable).
    // Current code keeps project label "other" with no warning. Desired: keep
    // base label "aegis.snapshot" + warn.
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
    // bugs-01 RED: base `Names` with `["prod-db"]`; project `Labeled` (same
    // rank, incomparable mode switch). Current code keeps project `Labeled`
    // with no warning. Desired: keep base `Names` + `["prod-db"]` + warn.
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
    // bugs-01 GREEN-BY-DESIGN guard: base Names `["a"]`; project Names
    // `["a", "b"]` (literal-string superset — every base pattern is in the
    // overlay). Broaden-or-equal is permitted: keep project value, NO warning.
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
    // bugs-01 GREEN-BY-DESIGN guard: base default `Labeled` (label
    // "aegis.snapshot"); project `Labeled` with the SAME label. Identical
    // effective scope — keep project value, NO warning.
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

// ── bugs-02: do not ratchet under SnapshotPolicy::None ────────────────────
// Under `SnapshotPolicy::None` the registry materializes NO providers, so the
// provider target ratchet must not fire (no spurious keep-base / no spurious
// warning). `provider_enabled_in_base` must be `base.snapshot_policy != None
// && (Full || flag)`.

#[test]
fn project_provider_target_not_ratcheted_under_snapshot_policy_none() {
    // bugs-02 RED: global `snapshot_policy = "None"` + `auto_snapshot_postgres
    // = true` + `[postgres_snapshot] database = "mydb"`; project empties the
    // database. Under None the postgres provider is NEVER materialized, so the
    // ratchet must NOT fire: keep project value "" + NO warning. Current code
    // treats `auto_snapshot_postgres = true` as enabling the provider
    // regardless of policy → keeps "mydb" + warns → test FAILS.
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

// ── regressions-01: reordered equal allow_write subset must NOT warn ───────
// The allow_write warning branch gates on `kept != requested` compared as
// DEBUG STRINGS. A reordered-but-equal project subset (base `["/opt","/tmp"]`,
// project `["/tmp","/opt"]`) yields `kept = ["/opt","/tmp"]` (base order) vs
// `requested = ["/tmp","/opt"]` → Debug strings differ → SPURIOUS warning
// though nothing was weakened. Desired: compare as sets; no warning when the
// project requested nothing outside the base.

#[test]
fn project_sandbox_allow_write_reordered_subset_no_warning() {
    // regressions-01 RED: global `allow_write = ["/opt", "/tmp"]`; project
    // `allow_write = ["/tmp", "/opt"]` (reordered equal set). Effective
    // `allow_write` must equal the set `{"/opt", "/tmp"}` (order-insensitive)
    // AND there must be NO `sandbox.allow_write` warning. Current code
    // spuriously warns because the Debug strings differ in order → FAILS.
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

    let mut effective = config.sandbox.allow_write.clone();
    effective.sort();
    let mut expected = vec![
        std::path::PathBuf::from("/opt"),
        std::path::PathBuf::from("/tmp"),
    ];
    expected.sort();
    assert_eq!(
        effective, expected,
        "reordered equal allow_write set must be preserved as a set; got {:?}",
        config.sandbox.allow_write
    );
    assert_no_warning_for(
        &warnings,
        "sandbox.allow_write",
        "regressions-01 reordered equal subset no warning",
    );
}

// ── tests-01: backfill missing GREEN-BY-DESIGN repoint guards ─────────────
// Pin already-correct repoint behavior for mysql and supabase so the bugs-01 /
// bugs-02 fixes cannot regress them.

#[test]
fn project_mysql_snapshot_cannot_repoint_database_when_enabled() {
    // #269: global `auto_snapshot_mysql = true` + `[mysql_snapshot] database
    // = "mydb"`; a project that repoints to "otherdb" is a decoy-database
    // attempt — Rollback must still restore into the trusted target, so the
    // base database is kept and a `mysql_snapshot.database` warning fires.
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
    // #269: host/user are Snapshot-target fields too — a project that leaves
    // `database` alone but repoints `host`/`user` still aims Rollback at a
    // different server, so each field is protected and warned individually.
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
    // #269: global `auto_snapshot_supabase = true` + `[supabase_snapshot.db]
    // database = "supadb"`; a project that repoints `db.database` is exactly
    // the decoy-database bypass this ticket closes — keep the base database
    // and warn on the dotted field name.
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
    // #269: `project_ref` and `db.host` are Snapshot-target fields too.
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
    // #269 / the issue's core scenario: global enables Supabase with the
    // target-match check on; a hostile project turns the check off AND
    // repoints the target. Both must be rejected.
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
    // #269 counter-case: base leaves the provider off, so the project is free
    // to enable its OWN Supabase target — but even then it may not disable
    // `require_config_target_match_on_rollback`, because that flag protects
    // whichever target ends up configured, including a project-chosen one.
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

    // The project's own database target is honored (base never enabled a target)...
    assert_eq!(config.supabase_snapshot.db.database, "projdb");
    // ...but the target-match check stays on regardless.
    assert!(config.supabase_snapshot.require_config_target_match_on_rollback);
    assert_has_warning_for(
        &warnings,
        "supabase_snapshot.require_config_target_match_on_rollback",
        "#269 rollback target-match unconditional",
    );
}

#[test]
fn global_layer_can_disable_supabase_rollback_target_match_and_repoint_target() {
    // #269 counterpart: the ratchet restricts only the PROJECT layer. The
    // global layer is trusted and stays last-wins for every Supabase field,
    // including `require_config_target_match_on_rollback` and the database
    // target itself — an operator can turn the check off or repoint the
    // target from their own global config.
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
    // No project `.aegis.toml` at all — this exercises defaults -> global only.

    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();

    assert!(!config.supabase_snapshot.require_config_target_match_on_rollback);
    assert_eq!(config.supabase_snapshot.project_ref, "global_ref");
    assert_eq!(config.supabase_snapshot.db.database, "globaldb");
    assert_eq!(config.supabase_snapshot.db.host, "global.supabase.co");
}