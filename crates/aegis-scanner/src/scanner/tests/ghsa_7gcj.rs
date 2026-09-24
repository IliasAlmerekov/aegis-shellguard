use super::*;

// GHSA-7gcj-4f7x-7fxj / #415: FS-001 and PS-006's regexes only fire when the
// recursive/force flag is the very first token after `rm`. Any other flag
// order — the flag trailing the operand, split across two tokens, preceded
// by an unrelated flag, delivered through a launcher or an absolute path, or
// reached through `;`/`&&` — was silently `Safe` (or `Danger` instead of
// `Block` for the root case). FS-020 and PS-008 are token-prefix rules that
// find the recursive flag anywhere in the invocation.

// ── Public issue #415: forms with no force flag, previously Safe ─────────

#[test]
fn assess_rm_dash_r_is_danger_via_fs020() {
    assert_assessment_matches_pattern("rm -r build", RiskLevel::Danger, "FS-020");
}

#[test]
fn assess_rm_dash_capital_r_is_danger_via_fs020() {
    assert_assessment_matches_pattern("rm -R build", RiskLevel::Danger, "FS-020");
}

#[test]
fn assess_rm_dash_dash_recursive_is_danger_via_fs020() {
    assert_assessment_matches_pattern("rm --recursive build", RiskLevel::Danger, "FS-020");
}

#[test]
fn assess_rm_dash_rv_bundle_is_danger_via_fs020() {
    assert_assessment_matches_pattern("rm -rv build", RiskLevel::Danger, "FS-020");
}

#[test]
fn assess_rm_dash_i_dash_r_is_danger_via_fs020() {
    assert_assessment_matches_pattern("rm -i -r build", RiskLevel::Danger, "FS-020");
}

#[test]
fn assess_rm_dash_r_double_dash_is_danger_via_fs020() {
    assert_assessment_matches_pattern("rm -r -- build", RiskLevel::Danger, "FS-020");
}

#[test]
fn assess_rm_flag_after_operand_is_danger_via_fs020() {
    assert_assessment_matches_pattern("rm build -r", RiskLevel::Danger, "FS-020");
}

#[test]
fn assess_command_launcher_rm_dash_r_is_danger_via_fs020() {
    assert_assessment_matches_pattern("command rm -r build", RiskLevel::Danger, "FS-020");
}

#[test]
fn assess_backslash_rm_dash_r_is_danger_via_fs020() {
    assert_assessment_matches_pattern(r"\rm -r build", RiskLevel::Danger, "FS-020");
}

#[test]
fn assess_absolute_path_rm_dash_r_is_danger_via_fs020() {
    assert_assessment_matches_pattern("/bin/rm -r build", RiskLevel::Danger, "FS-020");
}

#[test]
fn assess_chained_rm_dash_r_is_danger_via_fs020() {
    let s = scanner();
    let cmd = "echo hi; rm -r /tmp/blind && echo removed";
    let assessment = s.assess(cmd);

    assert_eq!(
        assessment.risk,
        RiskLevel::Danger,
        "command {cmd:?}: got {:?}",
        assessment.risk
    );
    assert!(
        assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "FS-020"),
        "command {cmd:?}: expected FS-020, got {:?}",
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}

// ── Advisory: force present too, but flag order still defeats FS-001 ─────

#[test]
fn assess_rm_dash_v_dash_rf_is_danger_via_fs020() {
    assert_assessment_matches_pattern("rm -v -rf build", RiskLevel::Danger, "FS-020");
}

#[test]
fn assess_rm_operand_then_dash_rf_is_danger_via_fs020() {
    assert_assessment_matches_pattern("rm build -rf", RiskLevel::Danger, "FS-020");
}

#[test]
fn assess_rm_operand_then_split_r_f_is_danger_via_fs020() {
    assert_assessment_matches_pattern("rm build -r -f", RiskLevel::Danger, "FS-020");
}

#[test]
fn assess_rm_dash_r_operand_dash_f_is_danger_via_fs020() {
    assert_assessment_matches_pattern("rm -r build -f", RiskLevel::Danger, "FS-020");
}

// ── FS-001 overlap: unchanged forms report FS-001 only ────────────────────

#[test]
fn assess_rm_dash_rf_still_reports_fs001_only() {
    let s = scanner();
    let assessment = s.assess("rm -rf build");

    assert!(
        assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "FS-001"),
        "expected FS-001 to fire for rm -rf build"
    );
    assert!(
        !assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "FS-020"),
        "FS-020 must not double-report a command FS-001 already covers, got {:?}",
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}

#[test]
fn assess_rm_dash_fr_still_reports_fs001_only() {
    let s = scanner();
    let assessment = s.assess("rm -fr build");

    assert!(
        assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "FS-001")
    );
    assert!(
        !assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "FS-020")
    );
}

#[test]
fn assess_rm_dash_r_dash_f_still_reports_fs001_only() {
    let s = scanner();
    let assessment = s.assess("rm -r -f build");

    assert!(
        assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "FS-001")
    );
    assert!(
        !assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "FS-020")
    );
}

// ── Must not fire ──────────────────────────────────────────────────────────

#[test]
fn assess_git_rm_dash_r_cached_stays_below_fs020() {
    let s = scanner();
    let assessment = s.assess("git rm -r --cached d");

    assert!(
        !assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "FS-020"),
        "git rm -r --cached must not trigger FS-020 (effective program is git, not rm)"
    );
}

#[test]
fn assess_gsutil_rm_dash_r_stays_below_fs020() {
    let s = scanner();
    let assessment = s.assess("gsutil rm -r gs://b");

    assert!(
        !assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "FS-020"),
        "gsutil rm -r must not trigger FS-020 (effective program is gsutil, not rm); CL-013 covers it"
    );
    assert!(
        assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "CL-013"),
        "gsutil rm -r must still trigger CL-013"
    );
}

#[test]
fn assess_rm_operand_after_double_dash_stays_safe() {
    let s = scanner();
    let assessment = s.assess("rm -- -r");

    assert!(
        !assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "FS-020"),
        "the operand after `--` is not a flag; got {:?}",
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}

#[test]
fn assess_rmdir_stays_below_fs020() {
    let s = scanner();
    let assessment = s.assess("rmdir d");

    assert!(
        !assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "FS-020")
    );
}

#[test]
fn assess_rm_plain_file_stays_below_fs020() {
    let s = scanner();
    let assessment = s.assess("rm file");

    assert!(
        !assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "FS-020")
    );
}

#[test]
fn assess_rm_dash_f_alone_stays_below_fs020() {
    let s = scanner();
    let assessment = s.assess("rm -f file");

    assert!(
        !assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "FS-020")
    );
}

#[test]
fn assess_rm_dash_i_alone_stays_below_fs020() {
    let s = scanner();
    let assessment = s.assess("rm -i file");

    assert!(
        !assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "FS-020")
    );
}

// ── PS-008: root deletion, any flag order (Block) ─────────────────────────
//
// PS-006's regex has the same first-token-only shape FS-001 had, so
// `rm -rf /` was Block only when the flags led the invocation. PS-008 is a
// token-prefix rule that finds the recursive flag anywhere and a `/`
// operand, independent of order or `-f`.

#[test]
fn assess_rm_dash_v_dash_rf_root_is_block_via_ps008() {
    assert_assessment_matches_pattern("rm -v -rf /", RiskLevel::Block, "PS-008");
}

#[test]
fn assess_rm_root_then_dash_rf_is_block_via_ps008() {
    assert_assessment_matches_pattern("rm / -rf", RiskLevel::Block, "PS-008");
}

#[test]
fn assess_rm_dash_r_root_is_block_via_ps008() {
    assert_assessment_matches_pattern("rm -r /", RiskLevel::Block, "PS-008");
}

#[test]
fn assess_rm_no_preserve_root_dash_rf_root_is_block_via_ps008() {
    assert_assessment_matches_pattern("rm --no-preserve-root -rf /", RiskLevel::Block, "PS-008");
}

#[test]
fn assess_rm_dash_v_dash_rf_no_preserve_root_is_block_via_ps008() {
    assert_assessment_matches_pattern("rm -v -rf --no-preserve-root /", RiskLevel::Block, "PS-008");
}

#[test]
fn assess_sudo_rm_dash_v_dash_rf_root_is_block_via_ps008() {
    assert_assessment_matches_pattern("sudo rm -v -rf /", RiskLevel::Block, "PS-008");
}

#[test]
fn assess_rm_dash_r_double_dash_root_is_block_via_ps008() {
    assert_assessment_matches_pattern("rm -r -- /", RiskLevel::Block, "PS-008");
}

// ── PS-008: spellings of `/` (Block) ──────────────────────────────────────
//
// `//`, `/.`, `/..`, and longer runs of those components all resolve to `/`.
// PS-006's regex only knows the literal `/`, so these fell back to Danger.

#[test]
fn assess_rm_dash_rf_double_slash_is_block_via_ps008() {
    assert_assessment_matches_pattern("rm -rf //", RiskLevel::Block, "PS-008");
}

#[test]
fn assess_rm_dash_rf_slash_dot_is_block_via_ps008() {
    assert_assessment_matches_pattern("rm -rf /.", RiskLevel::Block, "PS-008");
}

#[test]
fn assess_rm_dash_r_double_slash_is_block_via_ps008() {
    assert_assessment_matches_pattern("rm -r //", RiskLevel::Block, "PS-008");
}

#[test]
fn assess_rm_no_preserve_root_double_slash_is_block_via_ps008() {
    assert_assessment_matches_pattern("rm --no-preserve-root -rf //", RiskLevel::Block, "PS-008");
}

#[test]
fn assess_rm_dash_rf_mixed_root_components_is_block_via_ps008() {
    for command in [
        "rm -rf ///",
        "rm -rf /./",
        "rm -rf /..",
        "rm -rf /../.",
        "rm -rf //./",
    ] {
        assert_assessment_matches_pattern(command, RiskLevel::Block, "PS-008");
    }
}

#[test]
fn assess_rm_dash_rf_paths_below_root_stay_below_ps008() {
    let s = scanner();
    for command in [
        "rm -rf /.git",
        "rm -rf /...",
        "rm -rf //home",
        "rm -rf /./tmp",
        "rm -rf .",
    ] {
        let assessment = s.assess(command);
        assert!(
            !assessment
                .matched
                .iter()
                .any(|m| m.pattern.id.as_ref() == "PS-008"),
            "PS-008 must not fire for {command:?}, got {:?}",
            assessment
                .matched
                .iter()
                .map(|m| m.pattern.id.as_ref())
                .collect::<Vec<_>>()
        );
    }
}

// ── PS-006 overlap: unchanged forms report PS-006 only ────────────────────

#[test]
fn assess_rm_dash_rf_root_still_reports_ps006_only() {
    let s = scanner();
    let assessment = s.assess("rm -rf /");

    assert!(
        assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "PS-006"),
        "expected PS-006 to fire for rm -rf /"
    );
    assert!(
        !assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "PS-008"),
        "PS-008 must not double-report a command PS-006 already covers, got {:?}",
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}

#[test]
fn assess_rm_dash_r_dash_f_root_still_reports_ps006_only() {
    let s = scanner();
    let assessment = s.assess("rm -r -f /");

    assert!(
        assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "PS-006")
    );
    assert!(
        !assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "PS-008")
    );
}

// ── PS-008: must not fire ──────────────────────────────────────────────────

#[test]
fn assess_rm_rf_root_glob_stays_below_ps008() {
    let s = scanner();
    let assessment = s.assess("rm -rf /*");

    assert!(
        !assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "PS-008"),
        "PS-008 must not handle /* — out of scope, same as PS-006"
    );
}

#[test]
fn assess_rm_root_without_recursive_flag_stays_below_ps008() {
    let s = scanner();
    let assessment = s.assess("rm /");

    assert!(
        !assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "PS-008")
    );
}
