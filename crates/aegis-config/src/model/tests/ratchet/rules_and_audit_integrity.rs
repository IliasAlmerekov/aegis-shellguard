
fn full_project_warnings(
    base: &AegisConfig,
    project_path: &std::path::Path,
) -> Vec<crate::model::ratchet::SecurityRatchetWarning> {
    let layer = ConfigLayerPath {
        source_layer: ConfigSourceLayer::Project,
        path: project_path.to_path_buf(),
    };
    AegisConfig::project_security_ratchet_warnings(base, &layer).unwrap_or_default()
}

#[test]
fn project_rules_allow_dropped_and_warned() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(global_dir.join(GLOBAL_CONFIG_FILE), "mode = \"Protect\"\n").unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[[rules]]\npattern = [\"terraform\"]\ndecision = \"allow\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert!(
        config
            .rules
            .iter()
            .all(|r| r.decision != PolicyRuleDecision::Allow),
        "project-layer [[rules]] Allow must be dropped at merge; got rules = {:?}",
        config.rules
    );
    assert!(
        config.rules.is_empty(),
        "with no base rules, the merged rules Vec must be empty after dropping the project Allow; got {:?}",
        config.rules
    );
    assert_has_warning_for(&warnings, "rules", "C3-residual Fix-1 project Allow dropped");
}

#[test]
fn project_rules_allow_dropped_preserves_base_allow() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "[[rules]]\npattern = [\"git\"]\ndecision = \"allow\"\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[[rules]]\npattern = [\"terraform\"]\ndecision = \"allow\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert!(
        config.rules.iter().any(|r| {
            r.decision == PolicyRuleDecision::Allow
                && matches!(
                    r.pattern.first(),
                    Some(PolicyPatternToken::Single(s)) if s == "git"
                )
        }),
        "global-layer [[rules]] Allow must survive the project merge; got rules = {:?}",
        config.rules
    );
    assert!(
        !config.rules.iter().any(|r| {
            matches!(
                r.pattern.first(),
                Some(PolicyPatternToken::Single(s)) if s == "terraform"
            )
        }),
        "project-layer [[rules]] Allow must be dropped; got rules = {:?}",
        config.rules
    );
    assert_has_warning_for(&warnings, "rules", "C3-residual Fix-1 project Allow dropped (base kept)");
}

#[test]
fn project_rules_prompt_honored_no_warning() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(global_dir.join(GLOBAL_CONFIG_FILE), "mode = \"Protect\"\n").unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[[rules]]\npattern = [\"terraform\"]\ndecision = \"prompt\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.rules.len(),
        1,
        "project Prompt rule must be honored (present in merged rules); got {:?}",
        config.rules
    );
    assert_eq!(config.rules[0].decision, PolicyRuleDecision::Prompt);
    assert_no_warning_for(&warnings, "rules", "C3-residual Fix-1 project Prompt honored");
}

#[test]
fn project_rules_block_honored_no_warning() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(global_dir.join(GLOBAL_CONFIG_FILE), "mode = \"Protect\"\n").unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[[rules]]\npattern = [\"terraform\"]\ndecision = \"block\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.rules.len(),
        1,
        "project Block rule must be honored (present in merged rules); got {:?}",
        config.rules
    );
    assert_eq!(config.rules[0].decision, PolicyRuleDecision::Block);
    assert_no_warning_for(&warnings, "rules", "C3-residual Fix-1 project Block honored");
}

#[test]
fn global_rules_allow_honored_no_warning() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "[[rules]]\npattern = [\"git\"]\ndecision = \"allow\"\n",
    )
    .unwrap();
    fs::write(workspace.path().join(PROJECT_CONFIG_FILE), "").unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.rules.len(),
        1,
        "global [[rules]] Allow must be honored; got {:?}",
        config.rules
    );
    assert_eq!(config.rules[0].decision, PolicyRuleDecision::Allow);
    assert_no_warning_for(&warnings, "rules", "C3-residual Fix-1 global Allow honored");
}

#[test]
fn project_rules_prompt_with_when_then_allow_dropped_and_warned() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(global_dir.join(GLOBAL_CONFIG_FILE), "mode = \"Protect\"\n").unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[[rules]]\n\
         pattern = [\"terraform\"]\n\
         decision = \"prompt\"\n\
         when = { env = \"HOME\", value = \"/root\", then = \"allow\" }\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert!(
        config.rules.iter().all(|r| {
            r.when
                .as_ref()
                .map(|w| w.then != PolicyRuleDecision::Allow)
                .unwrap_or(true)
        }),
        "project-layer [[rules]] with when.then = \"allow\" must be dropped at merge; \
         got rules = {:?}",
        config.rules,
    );
    assert!(
        config.rules.is_empty(),
        "with no base rules, the merged rules Vec must be empty after dropping the \
         project `when.then = allow` rule; got {:?}",
        config.rules,
    );
    assert_has_warning_for(
        &warnings,
        "rules",
        "C3-residual Fix-1 bypass: project prompt+when.then=allow must warn",
    );
}

#[test]
fn project_rules_block_with_when_then_allow_dropped_and_warned() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(global_dir.join(GLOBAL_CONFIG_FILE), "mode = \"Protect\"\n").unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[[rules]]\n\
         pattern = [\"terraform\"]\n\
         decision = \"block\"\n\
         when = { env = \"CI\", value = \"true\", then = \"allow\" }\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = project_ratchet_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert!(
        config.rules.iter().all(|r| {
            r.when
                .as_ref()
                .map(|w| w.then != PolicyRuleDecision::Allow)
                .unwrap_or(true)
        }),
        "project-layer [[rules]] block with when.then = \"allow\" must be dropped at merge; \
         got rules = {:?}",
        config.rules,
    );
    assert!(
        config.rules.is_empty(),
        "with no base rules, the merged rules Vec must be empty after dropping the \
         project block+when.then=allow rule; got {:?}",
        config.rules,
    );
    assert_has_warning_for(
        &warnings,
        "rules",
        "C3-residual Fix-1 bypass: project block+when.then=allow must warn",
    );
}

#[test]
fn project_audit_integrity_mode_off_weakened_from_chainsha256_kept_and_warned() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(global_dir.join(GLOBAL_CONFIG_FILE), "mode = \"Protect\"\n").unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[audit]\nintegrity_mode = \"Off\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = full_project_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.audit.integrity_mode,
        AuditIntegrityMode::ChainSha256,
        "project must NOT weaken integrity_mode from ChainSha256 to Off; got {:?}",
        config.audit.integrity_mode
    );

    let warning = warnings
        .iter()
        .find(|w| w.field == "audit.integrity_mode");
    let warning = warning.expect("expected an `audit.integrity_mode` ratchet warning");
    assert_eq!(
        warning.requested, "Off",
        "warning.requested must be the project-requested value `Off`"
    );
    assert_eq!(
        warning.kept, "ChainSha256",
        "warning.kept must be the effective merged value `ChainSha256`"
    );
}

#[test]
fn project_audit_integrity_mode_chainsha256_tightens_from_off_honored_no_warning() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "[audit]\nintegrity_mode = \"Off\"\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[audit]\nintegrity_mode = \"ChainSha256\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = full_project_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.audit.integrity_mode,
        AuditIntegrityMode::ChainSha256,
        "project tightening Off→ChainSha256 must be honored; got {:?}",
        config.audit.integrity_mode
    );
    assert!(
        !warnings
            .iter()
            .any(|w| w.field == "audit.integrity_mode"),
        "tightening must NOT warn; got warnings = {:?}",
        warnings
    );
}

#[test]
fn project_audit_integrity_mode_chainsha256_equal_no_warning() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(global_dir.join(GLOBAL_CONFIG_FILE), "mode = \"Protect\"\n").unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[audit]\nintegrity_mode = \"ChainSha256\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = full_project_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.audit.integrity_mode,
        AuditIntegrityMode::ChainSha256,
        "equal ChainSha256 must stay ChainSha256; got {:?}",
        config.audit.integrity_mode
    );
    assert!(
        !warnings
            .iter()
            .any(|w| w.field == "audit.integrity_mode"),
        "equal value must NOT warn; got warnings = {:?}",
        warnings
    );
}

#[test]
fn global_audit_integrity_mode_off_last_wins_no_warning() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "[audit]\nintegrity_mode = \"Off\"\n",
    )
    .unwrap();
    fs::write(workspace.path().join(PROJECT_CONFIG_FILE), "").unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = full_project_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.audit.integrity_mode,
        AuditIntegrityMode::Off,
        "global-layer integrity_mode = Off must be honored (last-wins); got {:?}",
        config.audit.integrity_mode
    );
    assert!(
        !warnings
            .iter()
            .any(|w| w.field == "audit.integrity_mode"),
        "global layer must NOT trigger a project ratchet warning; got warnings = {:?}",
        warnings
    );
}

#[test]
fn project_audit_integrity_mode_off_equal_to_off_base_no_warning() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "[audit]\nintegrity_mode = \"Off\"\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[audit]\nintegrity_mode = \"Off\"\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = full_project_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(
        config.audit.integrity_mode,
        AuditIntegrityMode::Off,
        "equal Off must stay Off; got {:?}",
        config.audit.integrity_mode
    );
    assert!(
        !warnings
            .iter()
            .any(|w| w.field == "audit.integrity_mode"),
        "equal Off must NOT warn; got warnings = {:?}",
        warnings
    );
}

#[test]
fn project_audit_rotation_truncation_attempt_is_dropped_and_warned() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "[audit]\nrotation_enabled = true\nmax_file_size_bytes = 2048\nretention_files = 7\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[audit]\nmax_file_size_bytes = 1\nretention_files = 1\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = full_project_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert_eq!(config.audit.max_file_size_bytes, 2048);
    assert_eq!(config.audit.retention_files, 7);
    assert!(warnings.iter().any(|warning| {
        warning.field == "audit.max_file_size_bytes"
            && warning.requested == "1"
            && warning.kept == "2048"
    }));
    assert!(warnings.iter().any(|warning| {
        warning.field == "audit.retention_files"
            && warning.requested == "1"
            && warning.kept == "7"
    }));
}

#[test]
fn project_cannot_enable_audit_rotation_disabled_globally() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "[audit]\nrotation_enabled = false\nmax_file_size_bytes = 2048\nretention_files = 7\n",
    )
    .unwrap();
    fs::write(
        workspace.path().join(PROJECT_CONFIG_FILE),
        "[audit]\nrotation_enabled = true\nmax_file_size_bytes = 4096\nretention_files = 8\n",
    )
    .unwrap();

    let base = load_global_base(home.path());
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let warnings = full_project_warnings(&base, &workspace.path().join(PROJECT_CONFIG_FILE));

    assert!(!config.audit.rotation_enabled);
    assert_eq!(config.audit.max_file_size_bytes, 4096);
    assert_eq!(config.audit.retention_files, 8);
    assert!(warnings.iter().any(|warning| {
        warning.field == "audit.rotation_enabled"
            && warning.requested == "true"
            && warning.kept == "false"
    }));
}

#[test]
fn global_audit_rotation_settings_remain_last_wins() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(GLOBAL_CONFIG_DIR);
    fs::create_dir_all(&global_dir).unwrap();

    fs::write(
        global_dir.join(GLOBAL_CONFIG_FILE),
        "[audit]\nrotation_enabled = true\nmax_file_size_bytes = 1\nretention_files = 1\ncompress_rotated = false\n",
    )
    .unwrap();

    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();

    assert!(config.audit.rotation_enabled);
    assert_eq!(config.audit.max_file_size_bytes, 1);
    assert_eq!(config.audit.retention_files, 1);
    assert!(!config.audit.compress_rotated);
}
