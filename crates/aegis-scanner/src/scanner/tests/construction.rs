use super::*;

#[test]
fn malformed_builtin_sourced_regex_fails_construction_not_scan() {
    let bad = Pattern {
        id: "TEST-BAD-BUILTIN".into(),
        category: Category::Process,
        risk: RiskLevel::Danger,
        pattern: r"badregex\s+(unterminated".into(),
        description: "deliberately malformed built-in-sourced regex".into(),
        safe_alt: None,
        justification: None,
        source: PatternSource::Builtin,
    };

    let patterns = PatternSet::from_sources(&[bad])
        .expect("field validation passes; regex validity is not checked at this layer");
    let result = Scanner::try_new(patterns);

    assert!(
        matches!(
            result,
            Err(crate::ScannerError::InvalidPattern { ref id, .. }) if id == "TEST-BAD-BUILTIN"
        ),
        "a malformed built-in-sourced regex must fail construction, not scan time"
    );
}

#[test]
fn pattern_with_no_extractable_keyword_is_always_a_full_scan_candidate() {
    let custom = Pattern {
        id: "TEST-UNCOVERED".into(),
        category: Category::Process,
        risk: RiskLevel::Warn,
        pattern: ".*".into(),
        description: "keyword-less pattern must always be a full_scan candidate".into(),
        safe_alt: None,
        justification: None,
        source: PatternSource::Custom,
    };

    let patterns = PatternSet::from_sources(&[custom]).expect("field validation passes");
    let scanner = Scanner::try_new(patterns).expect("valid regex compiles eagerly");

    let assessment = scanner.assess("totally unrelated benign text");
    assert!(
        assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "TEST-UNCOVERED"),
        "a pattern with no extractable keyword must be evaluated regardless of \
         any keyword match"
    );
}

#[test]
fn sh_dash_c_is_flagged_by_exec_006() {
    let scanner = Scanner::try_new(PatternSet::load().expect("patterns.toml must load"))
        .expect("built-in patterns compile");

    let assessment = scanner.assess("sh -c id");

    assert_eq!(
        assessment.risk,
        RiskLevel::Warn,
        "`sh -c id` must be Warn, not Safe"
    );
    assert!(
        assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "EXEC-006"),
        "EXEC-006 (shell indirection) must be among the matched patterns: {:?}",
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}
