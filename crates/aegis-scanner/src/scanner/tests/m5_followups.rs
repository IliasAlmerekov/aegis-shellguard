use super::*;

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
