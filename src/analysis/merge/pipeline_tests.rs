use aegis_language::SourceLanguage;
use aegis_language::languages::python::analyze;
use aegis_types::{AnalysisStatus, DegradationReason, RiskLevel, SourceOrigin};

use super::merge_analysis;
use super::tests::baseline;
use crate::analysis::mapping::map_adapter_result;

#[test]
fn merge_lifts_baseline_safe_to_danger_and_carries_status() {
    let outcome = map_adapter_result(
        &analyze("subprocess.run(cmd)"),
        SourceLanguage::Python,
        SourceOrigin::Inline,
        None,
        None,
        0,
    );
    let merged = merge_analysis(&baseline(RiskLevel::Safe, Vec::new()), &outcome.analysis);

    assert_eq!(
        merged.risk,
        RiskLevel::Danger,
        "risk never decreases: a Danger language Match lifts Safe → Danger",
    );
    assert!(
        merged
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "LANG-EXEC"),
        "the language Match is appended to the merged Assessment",
    );
    let summary = merged
        .analysis
        .as_ref()
        .expect("merge carries the analysis summary");
    assert_eq!(summary.status, AnalysisStatus::Degraded);
    assert!(
        summary
            .degradation_reasons
            .contains(&DegradationReason::DynamicSource),
    );
}
