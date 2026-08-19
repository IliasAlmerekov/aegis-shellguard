use super::*;

// M5.3 — FS-019: recursive chmod over a system root. The rule is a
// token-prefix rule (ADR-014), but it keys on the target path rather than the
// mode: any recursive permission rewrite of a system root can destroy boot,
// service, or authentication behaviour.
#[test]
fn assess_m5_fs019_recursive_chmod_system_roots_fire() {
    let cases: &[(&str, RiskLevel, &str)] = &[
        ("chmod -R 000 /", RiskLevel::Danger, "FS-019"),
        ("chmod -R 755 /usr", RiskLevel::Danger, "FS-019"),
        ("chmod -R 700 /etc", RiskLevel::Danger, "FS-019"),
        ("chmod -Rf 000 /bin", RiskLevel::Danger, "FS-019"),
        ("chmod --recursive 000 /sbin", RiskLevel::Danger, "FS-019"),
        ("chmod -R 000 /lib", RiskLevel::Danger, "FS-019"),
        ("chmod -R 000 /var", RiskLevel::Danger, "FS-019"),
        ("chmod -R 000 /boot", RiskLevel::Danger, "FS-019"),
        // The trailing slash spelling is deliberately local to FS-019.
        ("chmod -R 000 /usr/", RiskLevel::Danger, "FS-019"),
        // Effective-program normalization and segmentation (ADR-014).
        ("/usr/bin/chmod -R 000 /", RiskLevel::Danger, "FS-019"),
        ("sudo chmod -R 000 /", RiskLevel::Danger, "FS-019"),
        ("rtk chmod -R 000 /", RiskLevel::Danger, "FS-019"),
        ("echo ok && chmod -R 000 /", RiskLevel::Danger, "FS-019"),
        // FS-019 overlaps PS-005 at the same risk level by design.
        ("chmod -R 777 /", RiskLevel::Danger, "FS-019"),
    ];

    for (cmd, risk, id) in cases {
        assert_assessment_matches_pattern(cmd, *risk, id);
    }

    let overlap = scanner().assess("chmod -R 777 /");
    for id in ["FS-019", "PS-005"] {
        assert!(
            overlap
                .matched
                .iter()
                .any(|matched| matched.pattern.id.as_ref() == id),
            "chmod -R 777 / must retain {id}: {:?}",
            overlap
                .matched
                .iter()
                .map(|matched| matched.pattern.id.as_ref())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn assess_m5_ps005_is_filesystem_without_changing_its_match() {
    let assessment = scanner().assess("chmod 777 /");
    let ps005 = assessment
        .matched
        .iter()
        .find(|matched| matched.pattern.id.as_ref() == "PS-005")
        .expect("chmod 777 / must keep matching PS-005");

    assert_eq!(ps005.pattern.category, Category::Filesystem);
}

// M5.4 — DB-009: destructive SQL `TRUNCATE` without the `TABLE` keyword.
// Positive (must-fire) cases. The rule is a match-anywhere regex `Pattern`
// (ADR-015): a SQL verb is an argument to a database client, never the
// `Effective program`, so it fires on `-c`/`-e`/`--command`/`--execute`/
// heredoc/stdin/`;`-compound delivery without enumerating clients. It fires
// only on forms carrying a SQL-only anchor: the `ONLY` keyword, a
// `CASCADE`/`RESTRICT` referential action, a `RESTART`/`CONTINUE IDENTITY`
// clause, or a comma-separated table list followed by one of those strong
// anchors. Bare `TRUNCATE <ident>` and a bare comma list are declared
// uncovered forms (see `m5_gaps.rs`).
#[test]
fn assess_m5_db009_truncate_without_table_fires() {
    let cases: &[(&str, RiskLevel, &str)] = &[
        // ONLY keyword (psql delivery)
        ("psql -c 'TRUNCATE ONLY users'", RiskLevel::Danger, "DB-009"),
        // ONLY with a CASCADE terminator
        ("TRUNCATE ONLY users CASCADE", RiskLevel::Danger, "DB-009"),
        // CASCADE / RESTRICT referential actions
        ("TRUNCATE users CASCADE", RiskLevel::Danger, "DB-009"),
        ("TRUNCATE users RESTRICT", RiskLevel::Danger, "DB-009"),
        // RESTART / CONTINUE IDENTITY clauses
        (
            "TRUNCATE users RESTART IDENTITY",
            RiskLevel::Danger,
            "DB-009",
        ),
        (
            "TRUNCATE users CONTINUE IDENTITY",
            RiskLevel::Danger,
            "DB-009",
        ),
        // schema-qualified and quoted identifiers
        ("TRUNCATE public.users CASCADE", RiskLevel::Danger, "DB-009"),
        ("TRUNCATE \"users\" CASCADE", RiskLevel::Danger, "DB-009"),
        // comma-separated list before a strong anchor
        (
            "TRUNCATE orders, order_items CASCADE",
            RiskLevel::Danger,
            "DB-009",
        ),
        // delivery: mysql -e and a ;-compound statement
        (
            "mysql -e 'TRUNCATE users CASCADE'",
            RiskLevel::Danger,
            "DB-009",
        ),
        (
            "psql -c 'SELECT 1; TRUNCATE users CASCADE'",
            RiskLevel::Danger,
            "DB-009",
        ),
        // overlap with DB-004 on TRUNCATE TABLE ONLY x — both Danger, same action
        ("TRUNCATE TABLE ONLY x", RiskLevel::Danger, "DB-009"),
    ];
    for (cmd, risk, id) in cases {
        assert_assessment_matches_pattern(cmd, *risk, id);
    }
}

// Heredoc delivery reaches the scanner through a distinct code path
// (heredoc-body extraction) from the `-c`/`-e` logical-segment path the other
// cases exercise. The `ONLY` terminator must accept the `\n` that ends the
// heredoc body line (ADR-015 delivery-agnostic guarantee).
#[test]
fn assess_m5_db009_truncate_only_via_heredoc_fires() {
    let s = scanner();
    let cmd = "psql <<EOF\nTRUNCATE ONLY users\nEOF";
    let assessment = s.assess(cmd);
    assert_eq!(
        assessment.risk,
        RiskLevel::Danger,
        "TRUNCATE ONLY in a psql heredoc body must reach Danger (got {:?})",
        assessment.risk
    );
    assert!(
        assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "DB-009"),
        "expected DB-009 for heredoc TRUNCATE ONLY, got {:?}",
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}

// M5.5 — DK-007: `docker volume rm` deletes a named volume. Positive
// (must-fire) cases. The rule is a token-prefix rule `[docker, volume, rm]`
// (ADR-014): the dangerous verb is the leading program, so a prefix match
// covers the tail. `-f` needs no special handling — a prefix rule matches the
// prefix and ignores the tail. Launcher/absolute-path delivery is covered by
// the ADR-014 table in `basic.rs`; the near-miss negatives live in
// `m5_gaps.rs`.
#[test]
fn assess_m5_docker_volume_rm_fires_dk007() {
    assert_assessment_matches_pattern("docker volume rm pgdata", RiskLevel::Danger, "DK-007");
    assert_assessment_matches_pattern("docker volume rm -f pgdata", RiskLevel::Danger, "DK-007");
    // Fires inside a compound command.
    assert_assessment_matches_pattern(
        "echo ok && docker volume rm pgdata",
        RiskLevel::Danger,
        "DK-007",
    );
}
