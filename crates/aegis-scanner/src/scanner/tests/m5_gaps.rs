use super::*;

// M5.4 — DB-009 narrowness guards. The positive (must-fire) cases live in
// `m5_followups.rs`; these guard the opposite direction — the near-miss
// invocations that must NOT raise DB-009 — so the rule stays narrow and
// fail-closed rather than fail-open. Bare `TRUNCATE <ident>` and a bare comma
// list without a strong clause are declared uncovered forms: the word collides
// with the coreutils command already covered by FS-006 and with ordinary prose,
// and neither carries a SQL-only anchor (ADR-015's tolerance for false
// positives rests on `drop` + `table` being a two-word anchor, which the bare
// form lacks).
#[test]
fn m5_db009_bare_truncate_ident_does_not_fire() {
    let s = scanner();
    let assessment = s.assess("TRUNCATE users");
    assert!(
        !assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "DB-009"),
        "DB-009 must not fire for bare 'TRUNCATE users': {:?}",
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
    assert!(
        assessment.risk < RiskLevel::Danger,
        "bare 'TRUNCATE users' must not reach Danger (got {:?})",
        assessment.risk
    );
}

// FS-006 covers the coreutils `truncate` command; DB-009 must not fire on a
// size-flag invocation, which is a file operation, not SQL.
#[test]
fn m5_db009_coreutils_truncate_stays_safe() {
    let s = scanner();
    for cmd in ["truncate -s 0 app.log", "truncate --size=0 app.log"] {
        let assessment = s.assess(cmd);
        assert!(
            !assessment
                .matched
                .iter()
                .any(|m| m.pattern.id.as_ref() == "DB-009"),
            "DB-009 must not fire for coreutils {cmd:?}: {:?}",
            assessment
                .matched
                .iter()
                .map(|m| m.pattern.id.as_ref())
                .collect::<Vec<_>>()
        );
        assert!(
            assessment.risk < RiskLevel::Danger,
            "coreutils {cmd:?} must not reach Danger (got {:?})",
            assessment.risk
        );
    }
}

// Ordinary prose must not raise DB-009. The comma and the word "only" are not
// SQL-specific anchors — `truncate logs, rebuild index` and `truncate only the
// last line` are the exact class of false positive that led to rejecting bare
// `TRUNCATE users` as a detection form.
#[test]
fn m5_db009_prose_does_not_fire() {
    let s = scanner();
    for cmd in [
        "git commit -m \"truncate long log lines\"",
        "git commit -m \"truncate logs, rebuild index\"",
        "echo \"we truncate stdout, then exit\"",
        "git commit -m \"truncate only the last line\"",
    ] {
        let assessment = s.assess(cmd);
        assert!(
            !assessment
                .matched
                .iter()
                .any(|m| m.pattern.id.as_ref() == "DB-009"),
            "DB-009 must not fire for prose {cmd:?}: {:?}",
            assessment
                .matched
                .iter()
                .map(|m| m.pattern.id.as_ref())
                .collect::<Vec<_>>()
        );
    }
}

// A bare comma-separated list without a strong clause is a declared uncovered
// form — no SQL-only anchor — the same category as bare `TRUNCATE users`. The
// comma alone is ordinary punctuation, not SQL syntax.
#[test]
fn m5_db009_bare_comma_list_does_not_fire() {
    let s = scanner();
    let assessment = s.assess("TRUNCATE orders, order_items");
    assert!(
        !assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "DB-009"),
        "DB-009 must not fire for a bare comma list 'TRUNCATE orders, order_items': {:?}",
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
    assert!(
        assessment.risk < RiskLevel::Danger,
        "bare comma list must not reach Danger (got {:?})",
        assessment.risk
    );
}

// Identifiers containing spaces inside quotes are a stated limit: the regex
// identifier class `[^\s,]+` cannot span a space, so a quoted identifier with
// an embedded space is not detected. Pinned here as a must-not-fire example so
// the limit is written down rather than left to be discovered.
#[test]
fn m5_db009_spaced_quoted_identifier_does_not_fire() {
    let s = scanner();
    let assessment = s.assess("TRUNCATE \"my table\" CASCADE");
    assert!(
        !assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "DB-009"),
        "DB-009 must not fire for a spaced quoted identifier 'TRUNCATE \"my table\" CASCADE': {:?}",
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}
