//! The result of assessing a shell command through the scanner pipeline.
//!
//! These are the *data* types produced by the scanner. The scanning logic that
//! builds them lives in the scanner layer; only the shapes live here.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::analysis::MatchEvidence;
use crate::command::ParsedCommand;
use crate::pattern::{Pattern, PatternSource};
use crate::risk::RiskLevel;

/// A concrete byte range inside the original command for confirmation UI highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightRange {
    /// Inclusive start byte offset.
    pub start: usize,
    /// Exclusive end byte offset.
    pub end: usize,
}

/// The stable, source-free label every language-aware [`MatchResult`] carries
/// in place of inspected source bytes (ADR-022 §10). Shared by
/// [`crate::analysis::language_match`], which stamps it at construction, and
/// [`MatchResult::public_matched_text`], which projects it as defense in depth
/// for manually constructed matches — kept in one place so the two never
/// drift apart.
pub const LANGUAGE_AWARE_MATCH_LABEL: &str = "language-aware operation";

/// A single pattern match with the actual text fragment that triggered it.
#[derive(Debug, Clone)]
pub struct MatchResult {
    /// The pattern that matched.
    pub pattern: Arc<Pattern>,
    /// A human-readable description of what matched.
    ///
    /// For language-aware matches this is always a stable source-free label;
    /// inspected source bytes never enter an `Assessment`.
    pub matched_text: String,
    /// The concrete span in the original command suitable for confirmation UI highlighting.
    pub highlight_range: Option<HighlightRange>,
    /// Typed evidence identifying this match's detection mechanism and source
    /// (ADR-022 §4). Populated by the scanner at construction; not projected
    /// into the v1 JSON / audit output, which keep their existing field shapes.
    pub evidence: MatchEvidence,
}

impl MatchResult {
    /// Return the match text safe to expose outside the assessment.
    ///
    /// Language-aware matches are already source-free at construction. This
    /// projection remains a defense in depth for manually constructed matches;
    /// persisted and machine-readable surfaces expose the operation,
    /// provenance metadata, and stable rule identifier instead of inspected
    /// source bytes (ADR-022 §10).
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

    /// Defense-in-depth regression: a hand-built `LanguageRule` match must be
    /// projected to the stable label even if its `matched_text` field somehow
    /// carries inspected source bytes (`language_match` itself never lets this
    /// happen — this test pins the second, independent safety net).
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

/// What ultimately caused the final interception decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecisionSource {
    /// Matched one or more built-in patterns compiled into the binary.
    BuiltinPattern,
    /// Matched one or more user-defined patterns from aegis.toml.
    CustomPattern,
    /// No patterns matched; the command was assessed Safe by default.
    Fallback,
}

/// What produced the final interception decision, expressed as the decisive
/// Matches (ADR-022 §4).
///
/// Replaces the singular [`DecisionSource`] concept: the decisive Matches are
/// *every* Match at the [`Assessment`]'s maximum [`RiskLevel`], not a single
/// label. `Fallback` is used only when no rule matched. The built-in vs custom
/// distinction lives per-Match (via detection evidence), not in the basis.
///
/// This is the audit-persistable shape (ADR-022 §10); the singular
/// `DecisionSource` is retained for v1 compatibility projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AssessmentBasis {
    /// No rule matched; the command was assessed `Safe` by fallback.
    Fallback,
    /// The decisive Matches — every Match at the Assessment's maximum `RiskLevel`.
    Decisive {
        /// IDs of every decisive Match, in `Assessment.matched` order.
        match_ids: Vec<String>,
    },
}

/// The default `AssessmentBasis` is `Fallback` — the conservative projection
/// for a v1 audit entry that predates the basis field (ADR-022 §10: interpret
/// absent v2 fields as legacy v1). `ScanExplanation::basis` is `#[serde(skip)]`,
/// so deserializing a v1 explanation fills `Fallback` here.
impl Default for AssessmentBasis {
    fn default() -> Self {
        AssessmentBasis::Fallback
    }
}

/// The result of assessing a shell command through the full scanner pipeline.
#[derive(Debug, Clone)]
pub struct Assessment {
    /// The highest `RiskLevel` among all matched patterns (`Safe` when none matched).
    pub risk: RiskLevel,
    /// Whether the command shape is `Effect-opaque execution` — it hands control
    /// to another execution layer (a script file, interpreter stdin, or a
    /// pipe-to-shell sink) whose eventual filesystem/database/network effect
    /// is not visible in argv. Orthogonal to `risk`: an effect-opaque command
    /// does not raise `RiskLevel` by itself; it only requests a recovery
    /// backstop downstream (ADR-016).
    pub effect_opaque: bool,
    /// Every pattern that matched the command (raw + inline scripts).
    pub matched: Vec<MatchResult>,
    /// Sorted, merged highlight spans for the original raw command.
    pub highlight_ranges: Vec<HighlightRange>,
    /// The parsed representation of the original command string.
    pub command: ParsedCommand,
    /// Language-aware analysis summary, set by [`crate::merge_analysis`] when a
    /// language result is merged into this `Assessment` (ADR-022 §1, §5). `None`
    /// on a baseline-only scanner `Assessment`. Orthogonal to `risk`:
    /// degradation may coexist with `Safe` and never authorizes auto-execution.
    /// `Assessment` itself does not derive `Serialize`/`Deserialize` (it holds
    /// `MatchResult`, which carries a non-serializable `Arc<Pattern>`); v1/v2
    /// audit projection is a separate explicit mapping (Iteration 2), so this
    /// field needs no `#[serde(skip)]`.
    pub analysis: Option<crate::analysis::AnalysisSummary>,
}

impl Assessment {
    /// Determine what caused this assessment, ignoring allowlist (handled by the caller).
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

    /// Return the [`AssessmentBasis`] — every decisive Match at this
    /// Assessment's maximum [`RiskLevel`], or [`AssessmentBasis::Fallback`] when
    /// no rule matched (ADR-022 §4).
    ///
    /// Decisive Matches are those whose `pattern.risk` equals `self.risk` (the
    /// scanner guarantees `self.risk` is the maximum matched risk, so this never
    /// drops a max-risk Match or includes a lower-risk one). IDs preserve
    /// `self.matched` order for deterministic output. This is the basis the audit
    /// v2 schema and the monotonic merge build on; the legacy
    /// [`Self::decision_source`] is kept only for v1 compatibility projection.
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

    /// Build an Assessment whose `risk` is the max of `matches`' risks, mirroring
    /// the scanner invariant (`risk == max(matched.risk)` or `Safe` when empty).
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
        // Two Danger Matches plus one lower-risk Warn Match. The basis must
        // retain BOTH decisive (Danger) IDs — this is the property the singular
        // DecisionSource loses (it collapses to a single "builtin_pattern"
        // label). ADR-022 §4: "all decisive Match IDs at the Assessment's
        // maximum RiskLevel".
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
        // A rule that DID match — even at Safe — is not Fallback. ADR-022 §4:
        // "Fallback ... when nothing matched". A custom Safe-risk rule is a
        // legal (if unusual) user pattern; its match must surface as Decisive.
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
                // Insertion (matched) order, not sorted — deterministic for audit.
                assert_eq!(match_ids, vec!["AAA".to_string(), "BBB".to_string()]);
            }
            other => panic!("expected Decisive, got {other:?}"),
        }
    }

    #[test]
    fn basis_round_trips_through_serde_for_audit_v2() {
        // ADR-022 §10 persists Assessment basis; pin the serialized shape.
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
