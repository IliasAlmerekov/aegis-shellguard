use std::collections::HashSet;

use aegis_types::{AnalysisStatus, AnalysisSummary, Assessment, DegradationReason, MatchResult};

#[derive(Debug, Clone)]
pub struct LanguageAnalysisResult {
    pub status: AnalysisStatus,
    pub matches: Vec<MatchResult>,
    pub degradation_reasons: Vec<DegradationReason>,
}

pub(super) fn merge_analysis(
    baseline: &Assessment,
    language: &LanguageAnalysisResult,
) -> Assessment {
    let mut merged_matched = baseline.matched.clone();
    let baseline_ids: HashSet<&str> = baseline
        .matched
        .iter()
        .map(|m| m.pattern.id.as_ref())
        .collect();
    for m in &language.matches {
        if !baseline_ids.contains(m.pattern.id.as_ref()) {
            merged_matched.push(m.clone());
        }
    }

    let risk = merged_matched
        .iter()
        .map(|m| m.pattern.risk)
        .chain(language.matches.iter().map(|m| m.pattern.risk))
        .max()
        .map(|mx| mx.max(baseline.risk))
        .unwrap_or(baseline.risk);

    Assessment {
        risk,
        effect_opaque: baseline.effect_opaque,
        matched: merged_matched,
        highlight_ranges: baseline.highlight_ranges.clone(),
        command: baseline.command.clone(),
        analysis: Some(AnalysisSummary {
            status: language.status,
            degradation_reasons: language.degradation_reasons.clone(),
        }),
    }
}

#[cfg(all(test, feature = "language-analysis"))]
mod pipeline_tests;
#[cfg(test)]
mod tests;
