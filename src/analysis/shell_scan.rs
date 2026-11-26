use aegis_types::{AnalysisStatus, DegradationReason, LanguageAnalysisResult};

use aegis_scanner::Scanner;

pub const SHELL_SOURCE_MATCH_LABEL: &str = "shell command in analyzed source";

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
