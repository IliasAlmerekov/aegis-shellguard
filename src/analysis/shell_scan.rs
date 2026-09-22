//! Shell Scanner pass over analyzed Shell/Bash source (issue #383).
//!
//! The Bash adapter reports only the operations it models (deletes, permission
//! changes, file writes, execution sinks). A shell script file is different
//! from an inline `bash -c` body: the outer Scanner never saw its contents, so
//! a script that runs `git push --force` would otherwise look clean. Every Bash
//! target therefore also runs through the same Scanner the outer command used,
//! so a command inside a script gets the Matches it would get when typed.
//!
//! Scanner Matches keep their regex/keyword evidence, so policy treats them
//! like the typed command. Their `matched_text` and highlight spans point into
//! the script, not the outer command: both are replaced so no script contents
//! reach rendering or the Audit log (ADR-022 §10).

use aegis_types::{AnalysisStatus, DegradationReason, LanguageAnalysisResult};

use crate::interceptor::scanner::Scanner;

/// Source-free label carried by a Scanner Match found inside analyzed source.
pub const SHELL_SOURCE_MATCH_LABEL: &str = "shell command in analyzed source";

/// Scan Shell/Bash `source` with `scanner` and project the result into the
/// language-analysis vocabulary.
///
/// Status is `Complete` when the Scanner matched anything, else
/// `NotApplicable`. An effect-opaque command inside the source (for example a
/// nested `python3 other.py`) runs a payload this pass cannot see, so it
/// records [`DegradationReason::DynamicSource`] instead of claiming safety.
#[must_use]
pub fn scan_shell_source(scanner: &Scanner, source: &str) -> LanguageAnalysisResult {
    let assessment = scanner.assess(source);
    let matches: Vec<_> = assessment
        .matched
        .into_iter()
        .map(|mut matched| {
            matched.matched_text = SHELL_SOURCE_MATCH_LABEL.to_string();
            matched.highlight_range = None;
            matched
        })
        .collect();

    if assessment.effect_opaque {
        return LanguageAnalysisResult {
            status: AnalysisStatus::Degraded,
            matches,
            degradation_reasons: vec![DegradationReason::DynamicSource],
        };
    }

    let status = if matches.is_empty() {
        AnalysisStatus::NotApplicable
    } else {
        AnalysisStatus::Complete
    };
    LanguageAnalysisResult {
        status,
        matches,
        degradation_reasons: Vec::new(),
    }
}

/// Fold `other` into `into`: Matches concatenate, reasons deduplicate, and
/// status takes the more degraded of the two (`NotApplicable < Complete <
/// Degraded`).
pub fn fold_result(into: &mut LanguageAnalysisResult, other: LanguageAnalysisResult) {
    into.matches.extend(other.matches);
    for reason in other.degradation_reasons {
        if !into.degradation_reasons.contains(&reason) {
            into.degradation_reasons.push(reason);
        }
    }
    into.status = into.status.max(other.status);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtin() -> std::sync::Arc<Scanner> {
        aegis_scanner::scanner_for(&[]).expect("built-in patterns compile")
    }

    #[test]
    fn a_safe_script_is_not_applicable() {
        let result = scan_shell_source(&builtin(), "echo hello\nls -la\n");
        assert_eq!(result.status, AnalysisStatus::NotApplicable);
        assert!(result.matches.is_empty());
        assert!(result.degradation_reasons.is_empty());
    }

    #[test]
    fn a_dangerous_command_matches_without_leaking_source() {
        let source = "echo deploying\ngit push --force origin main\n";
        let result = scan_shell_source(&builtin(), source);
        assert_eq!(result.status, AnalysisStatus::Complete);
        assert!(
            result
                .matches
                .iter()
                .any(|m| m.pattern.id.as_ref() == "GIT-003"),
            "git push --force must match GIT-003"
        );
        for matched in &result.matches {
            assert_eq!(matched.matched_text, SHELL_SOURCE_MATCH_LABEL);
            assert!(matched.highlight_range.is_none());
        }
    }

    #[test]
    fn an_effect_opaque_command_degrades_as_dynamic_source() {
        let result = scan_shell_source(&builtin(), "python3 other.py\n");
        assert_eq!(result.status, AnalysisStatus::Degraded);
        assert_eq!(
            result.degradation_reasons,
            vec![DegradationReason::DynamicSource]
        );
    }

    #[test]
    fn fold_keeps_the_more_degraded_status_and_deduplicates_reasons() {
        let mut into = LanguageAnalysisResult {
            status: AnalysisStatus::Degraded,
            matches: Vec::new(),
            degradation_reasons: vec![DegradationReason::DynamicSource],
        };
        let other = scan_shell_source(&builtin(), "python3 other.py\n");
        fold_result(&mut into, other);
        assert_eq!(into.status, AnalysisStatus::Degraded);
        assert_eq!(
            into.degradation_reasons,
            vec![DegradationReason::DynamicSource]
        );

        let mut into = LanguageAnalysisResult {
            status: AnalysisStatus::NotApplicable,
            matches: Vec::new(),
            degradation_reasons: Vec::new(),
        };
        fold_result(&mut into, scan_shell_source(&builtin(), "rm -rf /\n"));
        assert_eq!(into.status, AnalysisStatus::Complete);
        assert!(!into.matches.is_empty());
    }
}
