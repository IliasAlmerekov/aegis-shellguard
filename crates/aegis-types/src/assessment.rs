use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::analysis::MatchEvidence;
use crate::command::ParsedCommand;
use crate::pattern::{Pattern, PatternSource};
use crate::risk::RiskLevel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightRange {
    pub start: usize,
    pub end: usize,
}

pub const LANGUAGE_AWARE_MATCH_LABEL: &str = "language-aware operation";

#[derive(Clone)]
pub struct MatchResult {
    pub pattern: Arc<Pattern>,
    pub matched_text: String,
    pub highlight_range: Option<HighlightRange>,
    pub evidence: MatchEvidence,
}

impl fmt::Debug for MatchResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MatchResult")
            .field("pattern", &self.pattern)
            .field("matched_text", &self.public_matched_text())
            .field("highlight_range", &self.highlight_range)
            .field("evidence", &self.evidence)
            .finish()
    }
}

impl MatchResult {
    #[must_use]
    pub fn public_matched_text(&self) -> &str {
        match &self.evidence {
            MatchEvidence::LanguageRule { .. } => LANGUAGE_AWARE_MATCH_LABEL,
            _ => &self.matched_text,
        }
    }
}

#[cfg(test)]
mod public_matched_text_tests {
    use std::sync::Arc;

    use crate::analysis::{
        AnalysisProvenance, AnalysisStatus, ByteSpan, DetectedOperation, DetectionSource,
        MatchEvidence, OperandCertainty, OperationKind, OperationModifiers, SourceOrigin,
    };
    use crate::pattern::{Category, Pattern, PatternSource};
    use crate::risk::RiskLevel;

    use super::{HighlightRange, LANGUAGE_AWARE_MATCH_LABEL, MatchResult};

    fn pattern() -> Arc<Pattern> {
        Arc::new(Pattern {
            id: "TEST-001".into(),
            category: Category::Filesystem,
            risk: RiskLevel::Danger,
            pattern: "".into(),
            description: "test".into(),
            safe_alt: None,
            justification: None,
            source: PatternSource::Builtin,
        })
    }

    #[test]
    fn hand_built_language_rule_match_is_projected_to_the_stable_label_even_if_matched_text_carries_source()
     {
        let operation = DetectedOperation {
            kind: OperationKind::FilesystemDelete,
            modifiers: OperationModifiers::default(),
            certainty: OperandCertainty::Known,
        };
        let provenance = AnalysisProvenance {
            language: Some("python".to_string()),
            source_origin: SourceOrigin::ScriptFile,
            rule_id: Some("LANG-FS-DEL".to_string()),
            operation: Some(operation.clone()),
            file_path: Some("checked.py".to_string()),
            source_hash: Some("0".repeat(64)),
            span: Some(ByteSpan {
                line: 1,
                column: 1,
                byte_start: 0,
                byte_end: 4,
            }),
            certainty: OperandCertainty::Known,
            status: AnalysisStatus::Complete,
            degradation_reason: None,
        };
        let m = MatchResult {
            pattern: pattern(),
            matched_text: "LANGUAGE_SOURCE_PRIVACY_SENTINEL_test".to_string(),
            highlight_range: Some(HighlightRange { start: 0, end: 4 }),
            evidence: MatchEvidence::LanguageRule {
                source: DetectionSource::Builtin,
                operation,
                provenance,
            },
        };
        assert_eq!(m.public_matched_text(), LANGUAGE_AWARE_MATCH_LABEL);
        assert!(
            !format!("{m:?}").contains("LANGUAGE_SOURCE_PRIVACY_SENTINEL_test"),
            "Debug must not leak manually supplied language-aware match text"
        );
    }

    #[test]
    fn non_language_match_projects_its_matched_text_verbatim() {
        let m = MatchResult {
            pattern: pattern(),
            matched_text: "rm -rf /".to_string(),
            highlight_range: None,
            evidence: MatchEvidence::RegexPattern {
                source: DetectionSource::Builtin,
            },
        };
        assert_eq!(m.public_matched_text(), "rm -rf /");
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecisionSource {
    BuiltinPattern,
    CustomPattern,
    Fallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
#[schemars(
    description = "What produced the final interception decision, expressed as the decisive
Matches.

Replaces the singular [`DecisionSource`] concept: the decisive Matches are
*every* Match at the [`Assessment`]'s maximum [`RiskLevel`], not a single
label. `Fallback` is used only when no rule matched. The built-in vs custom
distinction lives per-Match (via detection evidence), not in the basis.

This is the audit-persistable shape; [`DecisionSource`]
remains a live one-value label alongside this richer basis."
)]
pub enum AssessmentBasis {
    #[schemars(description = "No rule matched; the command was assessed `Safe` by fallback.")]
    #[default]
    Fallback,
    #[schemars(
        description = "The decisive Matches — every Match at the Assessment's maximum `RiskLevel`."
    )]
    Decisive {
        #[schemars(description = "IDs of every decisive Match, in `Assessment.matched` order.")]
        match_ids: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub struct Assessment {
    pub risk: RiskLevel,
    pub effect_opaque: bool,
    pub matched: Vec<MatchResult>,
    pub highlight_ranges: Vec<HighlightRange>,
    pub command: ParsedCommand,
    pub analysis: Option<crate::analysis::AnalysisSummary>,
}

impl Assessment {
    pub fn decision_source(&self) -> DecisionSource {
        if self.matched.is_empty() {
            return DecisionSource::Fallback;
        }
        if self
            .matched
            .iter()
            .any(|m| m.pattern.source == PatternSource::Custom)
        {
            DecisionSource::CustomPattern
        } else {
            DecisionSource::BuiltinPattern
        }
    }

    pub fn basis(&self) -> AssessmentBasis {
        if self.matched.is_empty() {
            return AssessmentBasis::Fallback;
        }
        let max = self.risk;
        let match_ids = self
            .matched
            .iter()
            .filter(|m| m.pattern.risk == max)
            .map(|m| m.pattern.id.to_string())
            .collect();
        AssessmentBasis::Decisive { match_ids }
    }
}

#[cfg(test)]
mod basis_tests {
    use std::borrow::Cow;
    use std::sync::Arc;

    use super::{Assessment, AssessmentBasis, HighlightRange, MatchResult};
    use crate::analysis::{DetectionSource, MatchEvidence};
    use crate::command::ParsedCommand;
    use crate::pattern::{Category, Pattern, PatternSource};
    use crate::risk::RiskLevel;

    fn pattern(id: &str, risk: RiskLevel, source: PatternSource) -> Arc<Pattern> {
        Arc::new(Pattern {
            id: Cow::Owned(id.to_string()),
            category: Category::Filesystem,
            risk,
            pattern: Cow::Borrowed(""),
            description: Cow::Borrowed("test pattern"),
            safe_alt: None,
            justification: None,
            source,
        })
    }

    fn matched(id: &str, risk: RiskLevel, source: PatternSource) -> MatchResult {
        MatchResult {
            pattern: pattern(id, risk, source),
            matched_text: String::new(),
            highlight_range: None,
            evidence: MatchEvidence::RegexPattern {
                source: DetectionSource::from(source),
            },
        }
    }

    fn empty_command() -> ParsedCommand {
        ParsedCommand {
            program: Some("echo".to_string()),
            argv: Vec::new(),
            normalized: "echo".to_string(),
            inline_scripts: Vec::new(),
            raw: "echo".to_string(),
        }
    }

    fn assessment(matches: Vec<MatchResult>) -> Assessment {
        let risk = matches
            .iter()
            .map(|m| m.pattern.risk)
            .max()
            .unwrap_or(RiskLevel::Safe);
        Assessment {
            risk,
            effect_opaque: false,
            matched: matches,
            highlight_ranges: Vec::<HighlightRange>::new(),
            command: empty_command(),
            analysis: None,
        }
    }

    #[test]
    fn basis_returns_fallback_when_nothing_matched() {
        let a = assessment(Vec::new());
        assert_eq!(a.basis(), AssessmentBasis::Fallback);
    }

    #[test]
    fn basis_retains_every_equally_decisive_match() {
        let a = assessment(vec![
            matched("FS-001", RiskLevel::Danger, PatternSource::Builtin),
            matched("FS-014", RiskLevel::Warn, PatternSource::Builtin),
            matched("DB-001", RiskLevel::Danger, PatternSource::Builtin),
        ]);
        match a.basis() {
            AssessmentBasis::Decisive { match_ids } => {
                assert_eq!(match_ids, vec!["FS-001".to_string(), "DB-001".to_string()]);
            }
            other => panic!("expected Decisive, got {other:?}"),
        }
    }

    #[test]
    fn basis_decisive_excludes_lower_risk_matches() {
        let a = assessment(vec![
            matched("FS-001", RiskLevel::Danger, PatternSource::Builtin),
            matched("FS-014", RiskLevel::Warn, PatternSource::Builtin),
        ]);
        match a.basis() {
            AssessmentBasis::Decisive { match_ids } => {
                assert_eq!(match_ids, vec!["FS-001".to_string()]);
            }
            other => panic!("expected Decisive, got {other:?}"),
        }
    }

    #[test]
    fn basis_fallback_only_when_no_rule_matched() {
        let a = assessment(vec![matched(
            "SAFE-RULE",
            RiskLevel::Safe,
            PatternSource::Custom,
        )]);
        match a.basis() {
            AssessmentBasis::Decisive { match_ids } => {
                assert_eq!(match_ids, vec!["SAFE-RULE".to_string()]);
            }
            AssessmentBasis::Fallback => panic!("a matched rule must not be Fallback"),
        }
    }

    #[test]
    fn basis_decisive_preserves_matched_order() {
        let a = assessment(vec![
            matched("AAA", RiskLevel::Danger, PatternSource::Builtin),
            matched("BBB", RiskLevel::Danger, PatternSource::Custom),
        ]);
        match a.basis() {
            AssessmentBasis::Decisive { match_ids } => {
                assert_eq!(match_ids, vec!["AAA".to_string(), "BBB".to_string()]);
            }
            other => panic!("expected Decisive, got {other:?}"),
        }
    }

    #[test]
    fn basis_round_trips_through_serde_for_audit_v2() {
        let decisive = AssessmentBasis::Decisive {
            match_ids: vec!["FS-001".to_string(), "DB-001".to_string()],
        };
        let json = serde_json::to_string(&decisive).unwrap();
        let back: AssessmentBasis = serde_json::from_str(&json).unwrap();
        assert_eq!(back, decisive);
        assert!(json.contains("\"kind\":\"decisive\""));
        assert!(json.contains("\"match_ids\""));

        let json_f = serde_json::to_string(&AssessmentBasis::Fallback).unwrap();
        assert_eq!(
            serde_json::from_str::<AssessmentBasis>(&json_f).unwrap(),
            AssessmentBasis::Fallback,
        );
        assert!(json_f.contains("\"kind\":\"fallback\""));
    }
}
