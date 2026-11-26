use super::*;

fn ids(assessment: &Assessment) -> Vec<&str> {
    assessment
        .matched
        .iter()
        .map(|m| m.pattern.id.as_ref())
        .collect()
}

#[test]
fn assess_dash_capital_c_reset_hard_warns_via_git001() {
    assert_assessment_matches_pattern("git -C . reset --hard HEAD~1", RiskLevel::Warn, "GIT-001");
}

#[test]
fn assess_dash_capital_c_clean_force_warns_via_git002() {
    assert_assessment_matches_pattern("git -C . clean -fd .", RiskLevel::Warn, "GIT-002");
}

#[test]
fn assess_dash_capital_c_push_force_warns_via_git003() {
    assert_assessment_matches_pattern(
        "git -C . push origin main --force",
        RiskLevel::Warn,
        "GIT-003",
    );
}

#[test]
fn assess_dash_capital_c_filter_branch_is_danger_via_git004() {
    assert_assessment_matches_pattern(
        "git -C . filter-branch --tree-filter 'rm -f secret.txt' HEAD",
        RiskLevel::Danger,
        "GIT-004",
    );
}

#[test]
fn assess_dash_capital_c_rebase_warns_via_git005() {
    assert_assessment_matches_pattern("git -C . rebase -i HEAD~3", RiskLevel::Warn, "GIT-005");
}

#[test]
fn assess_dash_capital_c_branch_dash_capital_d_warns_via_git006() {
    assert_assessment_matches_pattern("git -C . branch -D old", RiskLevel::Warn, "GIT-006");
}

#[test]
fn assess_dash_capital_c_checkout_dashdash_dot_warns_via_git007() {
    assert_assessment_matches_pattern("git -C . checkout -- .", RiskLevel::Warn, "GIT-007");
}

#[test]
fn assess_dash_capital_c_stash_drop_warns_via_git008() {
    assert_assessment_matches_pattern("git -C . stash drop", RiskLevel::Warn, "GIT-008");
}

#[test]
fn assess_dash_capital_c_push_delete_warns_via_git009() {
    assert_assessment_matches_pattern(
        "git -C . push origin --delete old-branch",
        RiskLevel::Warn,
        "GIT-009",
    );
}

#[test]
fn assess_dash_c_config_value_reset_hard_warns_via_git001() {
    assert_assessment_matches_pattern("git -c k=v reset --hard", RiskLevel::Warn, "GIT-001");
}

#[test]
fn assess_git_dir_glued_value_reset_hard_warns_via_git001() {
    assert_assessment_matches_pattern(
        "git --git-dir=.git reset --hard",
        RiskLevel::Warn,
        "GIT-001",
    );
}

#[test]
fn assess_git_dir_separate_value_reset_hard_warns_via_git001() {
    assert_assessment_matches_pattern(
        "git --git-dir .git reset --hard",
        RiskLevel::Warn,
        "GIT-001",
    );
}

#[test]
fn assess_work_tree_reset_hard_warns_via_git001() {
    assert_assessment_matches_pattern("git --work-tree x reset --hard", RiskLevel::Warn, "GIT-001");
}

#[test]
fn assess_no_pager_reset_hard_warns_via_git001() {
    assert_assessment_matches_pattern("git --no-pager reset --hard", RiskLevel::Warn, "GIT-001");
}

#[test]
fn assess_dash_capital_p_reset_hard_warns_via_git001() {
    assert_assessment_matches_pattern("git -P reset --hard", RiskLevel::Warn, "GIT-001");
}

#[test]
fn assess_chain_of_several_options_reset_hard_warns_via_git001() {
    assert_assessment_matches_pattern(
        "git -C . -c x=y --no-pager reset --hard",
        RiskLevel::Warn,
        "GIT-001",
    );
}

#[test]
fn assess_sudo_dash_capital_c_reset_hard_warns_via_git001() {
    assert_assessment_matches_pattern("sudo git -C . reset --hard", RiskLevel::Warn, "GIT-001");
}

#[test]
fn assess_dash_capital_c_repo_push_delete_warns_via_git009() {
    assert_assessment_matches_pattern(
        "git -C repo push --delete origin x",
        RiskLevel::Warn,
        "GIT-009",
    );
}

#[test]
fn assess_unlisted_option_immediately_before_subcommand_warns_via_git001() {
    assert_assessment_matches_pattern("git --newopt reset --hard", RiskLevel::Warn, "GIT-001");
}

#[test]
fn assess_unlisted_option_with_one_operand_before_subcommand_warns_via_git001() {
    assert_assessment_matches_pattern("git --newopt x reset --hard", RiskLevel::Warn, "GIT-001");
}

#[test]
fn assess_custom_regex_rule_fires_behind_dash_capital_c() {
    let custom = Pattern {
        id: "TEST-GHSA-7564-GIT".into(),
        category: Category::Git,
        risk: RiskLevel::Warn,
        pattern: r"^git\s+reset\s+--hard".into(),
        description: "test-only custom rule for git reset --hard".into(),
        safe_alt: None,
        justification: None,
        source: PatternSource::Custom,
    };
    let patterns = PatternSet::from_sources(&[custom]).expect("field validation passes");
    let scanner = Scanner::try_new(patterns).expect("valid regex compiles eagerly");

    let assessment = scanner.assess("git -C . reset --hard");

    assert!(
        ids(&assessment).contains(&"TEST-GHSA-7564-GIT"),
        "custom regex rule must fire behind a skipped git global option, got {:?}",
        ids(&assessment)
    );
}

#[test]
fn assess_dash_capital_c_status_stays_safe() {
    let s = scanner();
    let assessment = s.assess("git -C . status");
    assert_eq!(
        assessment.risk,
        RiskLevel::Safe,
        "git -C . status: got {:?} ({:?})",
        assessment.risk,
        ids(&assessment)
    );
}

#[test]
fn assess_no_pager_log_stays_safe() {
    let s = scanner();
    let assessment = s.assess("git --no-pager log");
    assert_eq!(assessment.risk, RiskLevel::Safe);
}

#[test]
fn assess_version_stays_safe() {
    let s = scanner();
    let assessment = s.assess("git --version");
    assert_eq!(assessment.risk, RiskLevel::Safe);
}

#[test]
fn assess_dash_c_core_editor_commit_stays_safe() {
    let s = scanner();
    let assessment = s.assess("git -c core.editor=vim commit");
    assert_eq!(
        assessment.risk,
        RiskLevel::Safe,
        "got {:?}",
        ids(&assessment)
    );
}

#[test]
fn assess_dash_capital_c_push_dry_run_delete_stays_safe() {
    let s = scanner();
    let assessment = s.assess("git -C . push --dry-run --delete origin x");
    assert!(
        !ids(&assessment).contains(&"GIT-009"),
        "a dry run deletes nothing on the remote, got {:?}",
        ids(&assessment)
    );
    assert_eq!(
        assessment.risk,
        RiskLevel::Safe,
        "got {:?}",
        ids(&assessment)
    );
}

#[test]
fn assess_help_reset_hard_warns_via_git001() {
    assert_assessment_matches_pattern("git --help reset --hard", RiskLevel::Warn, "GIT-001");
}

fn alternating_unlisted_option_command(pairs: usize) -> String {
    let mut parts = vec!["git".to_string()];
    for i in 0..pairs {
        parts.push(format!("--o{i}"));
        parts.push(format!("v{i}"));
    }
    parts.push("reset".to_string());
    parts.push("--hard".to_string());
    parts.join(" ")
}

#[test]
fn assess_at_the_candidate_cap_still_scans_and_warns_via_git001() {
    let cmd = alternating_unlisted_option_command(15);
    let assessment = scanner().assess(&cmd);

    assert_eq!(
        assessment.risk,
        RiskLevel::Warn,
        "{cmd}: got {:?}",
        ids(&assessment)
    );
    assert!(
        ids(&assessment).contains(&"GIT-001"),
        "16 candidates is still within the cap, got {:?}",
        ids(&assessment)
    );
    assert!(!ids(&assessment).contains(&"SCAN-004"));
}

#[test]
fn assess_one_past_the_candidate_cap_warns_via_scan004_instead_of_scanning() {
    let cmd = alternating_unlisted_option_command(16);
    let assessment = scanner().assess(&cmd);

    assert_eq!(
        assessment.risk,
        RiskLevel::Warn,
        "{cmd}: got {:?}",
        ids(&assessment)
    );
    assert!(
        ids(&assessment).contains(&"SCAN-004"),
        "17 candidates exceeds the cap, got {:?}",
        ids(&assessment)
    );
}

#[test]
fn assess_2000_unlisted_git_options_warns_via_scan004_without_scanning_candidates() {
    let cmd = alternating_unlisted_option_command(2000);
    let assessment = scanner().assess(&cmd);

    assert_eq!(
        assessment.risk,
        RiskLevel::Warn,
        "got {:?}",
        ids(&assessment)
    );
    assert!(
        ids(&assessment).contains(&"SCAN-004"),
        "got {:?}",
        ids(&assessment)
    );
    assert!(
        !ids(&assessment).contains(&"GIT-001"),
        "cap must skip candidate scanning entirely, got {:?}",
        ids(&assessment)
    );
}
