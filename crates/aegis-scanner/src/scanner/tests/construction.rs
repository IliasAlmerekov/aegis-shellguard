//! Issue #319: `Scanner` construction.
//!
//! These tests use `PatternSet::from_sources` with a hand-built `Pattern` to
//! reach cases the shipped `patterns.toml` doesn't exercise: a built-in-sourced
//! pattern with a regex that fails to compile, and a pattern with no
//! extractable keyword.

use super::*;

/// Regex compilation stays eager for every pattern regardless of source
/// (issue #319 — a lazy built-in path was tried and reverted; see
/// "Lazy built-in regex compilation reverted" in docs/performance-baseline.md).
/// A malformed `Builtin`-sourced regex must therefore fail construction with a
/// typed error, exactly like a malformed custom one, never surface later as a
/// scan-time surprise.
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

/// A pattern whose regex yields no extractable literal keyword (`has_uncovered`)
/// cannot be gated by AC keyword matching — it must always be a `full_scan`
/// candidate, regardless of what appeared in the command.
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

/// Regression for a false negative that keyword narrowing in `full_scan`
/// introduced on the #319 branch (removed before merge):
/// `find_embedded_literal` walking the `sh` branch of `EXEC-006`'s
/// alternation (`^sh\s+(?:--[a-z-]+\s+)*-[a-zA-Z]*c\b`) discards the
/// two-character literal `"sh"` against its three-character floor, keeps
/// scanning past the optional group's regex syntax, and picks up `":"`,
/// `"-"`, `"-"` as though the command had to contain that text — a keyword no
/// matching command actually contains. The narrowing used that keyword to decide
/// which regexes `full_scan` runs at all, so `EXEC-006` was silently skipped.
/// `full_scan` no longer narrows by keyword, so `sh -c id` must be caught
/// through the real, shipped `patterns.toml`, not a hand-built pattern.
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
