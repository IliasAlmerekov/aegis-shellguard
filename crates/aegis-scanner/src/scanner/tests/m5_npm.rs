use super::*;

// M5.6 — PKG-006 / PKG-007: outbound irreversible npm actions (issue #194).
//
// `npm publish` is Warn (a normal intended act that must not happen
// unattended); `npm unpublish` is Danger (it breaks every consumer depending on
// the version). Both are outbound irreversible actions rather than destruction
// of local state. PKG-006 carries the negative condition `--dry-run` — the
// rehearsal an agent runs first must stay Safe, in any flag position.

#[test]
fn assess_m5_pkg006_npm_publish_fires_warn() {
    let cases: &[(&str, RiskLevel, &str)] = &[
        ("npm publish", RiskLevel::Warn, "PKG-006"),
        ("npm publish --access public", RiskLevel::Warn, "PKG-006"),
        ("npm publish --tag next", RiskLevel::Warn, "PKG-006"),
    ];
    for (cmd, risk, id) in cases {
        assert_assessment_matches_pattern(cmd, *risk, id);
    }
}

#[test]
fn assess_m5_pkg006_dry_run_stays_safe_in_any_flag_position() {
    let s = scanner();
    for cmd in [
        "npm publish --dry-run",
        "npm publish --dry-run --access public",
        "npm publish --access public --dry-run",
    ] {
        let assessment = s.assess(cmd);
        assert_eq!(
            assessment.risk,
            RiskLevel::Safe,
            "npm publish --dry-run must stay Safe for {cmd:?} (got {:?})",
            assessment.risk
        );
        assert!(
            !assessment
                .matched
                .iter()
                .any(|m| m.pattern.id.as_ref() == "PKG-006"),
            "PKG-006 must not fire for {cmd:?}: {:?}",
            assessment
                .matched
                .iter()
                .map(|m| m.pattern.id.as_ref())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn assess_m5_pkg007_npm_unpublish_fires_danger() {
    assert_assessment_matches_pattern("npm unpublish", RiskLevel::Danger, "PKG-007");
    assert_assessment_matches_pattern("npm unpublish --force pkg", RiskLevel::Danger, "PKG-007");
    assert_assessment_matches_pattern("npm unpublish pkg@1.0.0", RiskLevel::Danger, "PKG-007");
}

#[test]
fn assess_m5_npm_rules_through_launchers_and_compound_fire() {
    // ADR-014: launcher stripping, absolute-path basename normalization, and
    // compound-command segmentation apply to these token-prefix rules too.
    assert_assessment_matches_pattern("sudo npm publish", RiskLevel::Warn, "PKG-006");
    assert_assessment_matches_pattern("rtk npm publish", RiskLevel::Warn, "PKG-006");
    assert_assessment_matches_pattern("/usr/bin/npm publish", RiskLevel::Warn, "PKG-006");
    assert_assessment_matches_pattern("echo ok && npm publish", RiskLevel::Warn, "PKG-006");
    assert_assessment_matches_pattern(
        "sudo npm unpublish --force pkg",
        RiskLevel::Danger,
        "PKG-007",
    );
}

#[test]
fn assess_m5_npm_ordinary_subcommands_stay_safe() {
    let s = scanner();
    for cmd in [
        "npm install",
        "npm install --global evil-pkg",
        "npm run build",
        "npm test",
        "npm view package",
    ] {
        let assessment = s.assess(cmd);
        assert_eq!(
            assessment.risk,
            RiskLevel::Safe,
            "{cmd:?} must stay Safe (got {:?})",
            assessment.risk
        );
    }
}
