//! Issue #319: `Scanner` construction and keyword-driven candidate selection.
//!
//! These tests use `PatternSet::from_sources` with a hand-built `Pattern` to
//! reach cases the shipped `patterns.toml` doesn't exercise: a built-in-sourced
//! pattern with a regex that fails to compile, a pattern with no extractable
//! keyword, and two patterns whose keywords overlap at the same byte range.

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

/// `full_scan`'s keyword pass must be overlapping. A non-overlapping pass over
/// keywords `"she"` and `"he"` against `"she is evil"` reports only `"she"`
/// (0..3) and then continues past it, skipping `"he"` at 1..3 even though it
/// is also present — dropping the pattern that owns it. That is exactly the
/// forbidden false negative.
#[test]
fn overlapping_keyword_search_finds_both_nested_patterns() {
    let outer = Pattern {
        id: "TEST-OVERLAP-SHE".into(),
        category: Category::Process,
        risk: RiskLevel::Warn,
        pattern: r"she\s+is\s+evil".into(),
        description: "outer keyword ('she')".into(),
        safe_alt: None,
        justification: None,
        source: PatternSource::Custom,
    };
    let inner = Pattern {
        id: "TEST-OVERLAP-HE".into(),
        category: Category::Process,
        risk: RiskLevel::Warn,
        pattern: r"he\s+is\s+evil".into(),
        description: "keyword ('he') nested inside the outer pattern's keyword".into(),
        safe_alt: None,
        justification: None,
        source: PatternSource::Custom,
    };

    let patterns = PatternSet::from_sources(&[outer, inner]).expect("field validation passes");
    let scanner = Scanner::try_new(patterns).expect("both regexes compile");

    let assessment = scanner.assess("she is evil");
    let ids: Vec<&str> = assessment
        .matched
        .iter()
        .map(|m| m.pattern.id.as_ref())
        .collect();

    assert!(
        ids.contains(&"TEST-OVERLAP-SHE"),
        "outer keyword pattern must match: {ids:?}"
    );
    assert!(
        ids.contains(&"TEST-OVERLAP-HE"),
        "nested keyword must not be dropped by a non-overlapping keyword pass: {ids:?}"
    );
}
