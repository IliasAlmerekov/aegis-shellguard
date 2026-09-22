#![deny(missing_docs)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

//! Command risk scanning for Aegis.
//!
//! This crate owns the [`Scanner`] (Aho-Corasick quick scan + regex full scan +
//! token-prefix matching) and the [`PatternSet`] that feeds it (built-in
//! patterns embedded from `patterns.toml`, merged with caller-supplied custom
//! patterns). It depends on `aegis-types` for the data vocabulary and
//! `aegis-parser` for tokenization. It is deliberately ignorant of where custom
//! patterns come from — callers convert their config types into [`Pattern`]
//! before handing them over.

mod error;
mod nested;
mod patterns;
mod scanner;

use std::sync::{Arc, LazyLock};

use aegis_types::{Assessment, Pattern};

pub use error::ScannerError;
pub use patterns::{PatternSet, PrefixRule};
pub use scanner::{Scanner, try_new_call_count_for_tests};

/// Process-wide built-in scanner, compiled once from the embedded
/// `patterns.toml`. The error is carried as a `String` (not `ScannerError`,
/// which does not implement `Clone`) so each access can rebuild a fresh typed
/// error without cloning the original.
static BUILTIN_SCANNER: LazyLock<Result<Arc<Scanner>, String>> = LazyLock::new(|| {
    PatternSet::load()
        .and_then(Scanner::try_new)
        .map(Arc::new)
        .map_err(|e| e.to_string())
});

/// Assess a command with the built-in scanner.
pub fn assess(cmd: &str) -> Result<Assessment, ScannerError> {
    Ok(builtin_scanner()?.assess(cmd))
}

/// Resolve the effective scanner for the provided custom-pattern set.
///
/// An empty slice returns a clone of the process-wide built-in-scanner `Arc`.
/// A non-empty slice builds a fresh scanner merging the built-in patterns with
/// `custom_patterns` and returns it wrapped in a new `Arc`. Callers own
/// keeping that `Arc` alive as long as the merged scanner is needed; nothing
/// here caches it.
pub fn scanner_for(custom_patterns: &[Pattern]) -> Result<Arc<Scanner>, ScannerError> {
    if custom_patterns.is_empty() {
        return builtin_scanner();
    }
    PatternSet::from_sources(custom_patterns)
        .and_then(Scanner::try_new)
        .map(Arc::new)
}

fn builtin_scanner() -> Result<Arc<Scanner>, ScannerError> {
    match &*BUILTIN_SCANNER {
        Ok(scanner) => Ok(Arc::clone(scanner)),
        Err(message) => Err(ScannerError::Build(message.clone())),
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use aegis_types::{Category, PatternSource};

    use super::*;

    #[test]
    fn assess_reports_safe_for_benign_command() {
        let assessment = assess("echo hello world").expect("built-in patterns compile");
        assert_eq!(assessment.risk, aegis_types::RiskLevel::Safe);
    }

    #[test]
    fn scanner_for_empty_slice_reuses_the_builtin_scanner() {
        let scanner = scanner_for(&[]).expect("built-in patterns compile");
        assert!(Arc::ptr_eq(&scanner, &builtin_scanner().unwrap()));
    }

    #[test]
    fn scanner_for_merges_custom_patterns_with_the_builtin_set() {
        let custom = Pattern {
            id: Cow::Borrowed("USR-REG-001"),
            category: Category::Cloud,
            risk: aegis_types::RiskLevel::Warn,
            pattern: Cow::Borrowed("internal-teardown"),
            description: Cow::Borrowed("Internal teardown guard"),
            safe_alt: Some(Cow::Borrowed("internal-teardown --dry-run")),
            justification: None,
            source: PatternSource::Custom,
        };

        let scanner = scanner_for(&[custom]).expect("custom pattern set compiles");
        let assessment = scanner.assess("internal-teardown && rm -rf /tmp/demo");

        assert_eq!(assessment.risk, aegis_types::RiskLevel::Danger);
        assert!(
            assessment
                .matched
                .iter()
                .any(|matched| matched.pattern.source == PatternSource::Custom)
        );
        assert!(
            assessment
                .matched
                .iter()
                .any(|matched| matched.pattern.source == PatternSource::Builtin)
        );
    }
}
