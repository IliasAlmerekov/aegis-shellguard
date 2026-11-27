use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::Arc;

use serde::Deserialize;

use aegis_types::{Category, Pattern, PatternSource, PatternToken, PrefixPattern};

use aegis_types::RiskLevel;

use crate::error::ScannerError;

#[derive(Debug, Clone)]
pub struct PrefixRule {
    pub id: Cow<'static, str>,
    pub category: Category,
    pub pattern: PrefixPattern,
    pub risk: RiskLevel,
    pub description: Cow<'static, str>,
    pub safe_alt: Option<Cow<'static, str>>,
    pub justification: Option<Cow<'static, str>>,
    pub source: PatternSource,
    pub suppressed_by: &'static [&'static str],
    pub match_examples: &'static [&'static str],
    pub not_match_examples: &'static [&'static str],
}

#[derive(Debug, Deserialize)]
struct RawPattern {
    id: String,
    category: Category,
    risk: RiskLevel,
    pattern: String,
    description: String,
    safe_alt: Option<String>,
    justification: Option<String>,
}

impl From<RawPattern> for Pattern {
    fn from(raw: RawPattern) -> Self {
        Pattern {
            id: Cow::Owned(raw.id),
            category: raw.category,
            risk: raw.risk,
            pattern: Cow::Owned(raw.pattern),
            description: Cow::Owned(raw.description),
            safe_alt: raw.safe_alt.map(Cow::Owned),
            justification: raw.justification.map(Cow::Owned),
            source: PatternSource::Builtin,
        }
    }
}

#[derive(Debug, Deserialize)]
struct PatternsFile {
    patterns: Vec<RawPattern>,
}

#[derive(Debug)]
pub struct PatternSet {
    patterns: Vec<Arc<Pattern>>,
    prefix_rules: Vec<Arc<PrefixRule>>,
}

const BUILTIN_PATTERNS_TOML: &str = include_str!("../patterns.toml");

impl PatternSet {
    pub fn load() -> Result<PatternSet, ScannerError> {
        Self::from_sources(&[])
    }

    pub fn from_sources(custom_patterns: &[Pattern]) -> Result<PatternSet, ScannerError> {
        let file: PatternsFile = toml::from_str(BUILTIN_PATTERNS_TOML)
            .map_err(|e| ScannerError::Build(format!("failed to parse patterns.toml: {e}")))?;

        let builtin_patterns: Vec<Pattern> = file.patterns.into_iter().map(Pattern::from).collect();

        let mut pattern_ids: HashSet<String> =
            HashSet::with_capacity(builtin_patterns.len() + custom_patterns.len());
        let mut patterns: Vec<Arc<Pattern>> =
            Vec::with_capacity(builtin_patterns.len() + custom_patterns.len());

        for pattern in builtin_patterns
            .into_iter()
            .chain(custom_patterns.iter().cloned())
        {
            Self::validate_pattern(&pattern, &mut pattern_ids)?;
            patterns.push(Arc::new(pattern));
        }

        let prefix_rules = builtin_prefix_rules();

        #[cfg(debug_assertions)]
        for rule in &prefix_rules {
            rule.validate_examples().map_err(|e| {
                ScannerError::Build(format!(
                    "built-in prefix rule {} failed example validation: {e}",
                    rule.id
                ))
            })?;
        }

        for rule in &prefix_rules {
            Self::validate_prefix_rule(rule, &pattern_ids)?;
        }

        let prefix_rules: Vec<Arc<PrefixRule>> = prefix_rules.into_iter().map(Arc::new).collect();

        Ok(PatternSet {
            patterns,
            prefix_rules,
        })
    }

    pub fn patterns(&self) -> &[Arc<Pattern>] {
        self.patterns.as_slice()
    }

    pub fn prefix_rules(&self) -> &[Arc<PrefixRule>] {
        self.prefix_rules.as_slice()
    }

    fn validate_pattern(pattern: &Pattern, ids: &mut HashSet<String>) -> Result<(), ScannerError> {
        if pattern.id.trim().is_empty() {
            return Err(ScannerError::InvalidPattern {
                id: pattern.id.to_string(),
                reason: format!("empty id (source={:?})", pattern.source),
            });
        }

        if pattern.pattern.trim().is_empty() {
            return Err(ScannerError::InvalidPattern {
                id: pattern.id.to_string(),
                reason: "empty regex pattern".to_string(),
            });
        }

        if pattern.description.trim().is_empty() {
            return Err(ScannerError::InvalidPattern {
                id: pattern.id.to_string(),
                reason: "empty description".to_string(),
            });
        }

        let id = pattern.id.as_ref();
        if !ids.insert(id.to_string()) {
            return Err(ScannerError::InvalidPattern {
                id: id.to_string(),
                reason: "duplicate pattern id is not allowed".to_string(),
            });
        }

        Ok(())
    }

    fn validate_prefix_rule(
        rule: &PrefixRule,
        pattern_ids: &HashSet<String>,
    ) -> Result<(), ScannerError> {
        if rule.id.trim().is_empty() {
            return Err(ScannerError::InvalidPattern {
                id: rule.id.to_string(),
                reason: format!("empty prefix rule id (source={:?})", rule.source),
            });
        }

        match rule.pattern.first() {
            Some(PatternToken::Single(_)) | Some(PatternToken::Alts(_)) => {}
            Some(other) => {
                return Err(ScannerError::InvalidPattern {
                    id: rule.id.to_string(),
                    reason: format!(
                        "prefix rule first token must be Single or Alts (names a program), got {other:?}"
                    ),
                });
            }
            None => {
                return Err(ScannerError::InvalidPattern {
                    id: rule.id.to_string(),
                    reason: "empty prefix rule pattern".to_string(),
                });
            }
        }

        if rule.description.trim().is_empty() {
            return Err(ScannerError::InvalidPattern {
                id: rule.id.to_string(),
                reason: "empty prefix rule description".to_string(),
            });
        }

        if let Some(blank) = rule
            .suppressed_by
            .iter()
            .find(|token| token.trim().is_empty())
        {
            return Err(ScannerError::InvalidPattern {
                id: rule.id.to_string(),
                reason: format!("blank suppressing token {blank:?} in suppressed_by"),
            });
        }

        let id = rule.id.as_ref();
        if pattern_ids.contains(id) {
            return Err(ScannerError::InvalidPattern {
                id: id.to_string(),
                reason: "prefix rule id conflicts with an existing regex pattern id".to_string(),
            });
        }

        Ok(())
    }
}

mod builtins_a;
mod builtins_b;

pub(super) fn s(s: &'static str) -> PatternToken {
    PatternToken::Single(Cow::Borrowed(s))
}

pub(super) fn a(alts: &'static [&'static str]) -> PatternToken {
    PatternToken::Alts(alts.iter().map(|&s| Cow::Borrowed(s)).collect())
}

pub(super) fn any_star() -> PatternToken {
    PatternToken::AnyStar
}

fn builtin_prefix_rules() -> Vec<PrefixRule> {
    let mut rules = builtins_a::rules();
    rules.extend(builtins_b::rules());
    rules
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::RegexBuilder;
    use std::collections::HashSet;

    #[test]
    fn every_builtin_pattern_regex_compiles() {
        let set = PatternSet::load().expect("patterns.toml must load");
        for pattern in set.patterns() {
            RegexBuilder::new(pattern.pattern.as_ref())
                .case_insensitive(true)
                .build()
                .unwrap_or_else(|e| {
                    panic!("built-in pattern {} has an invalid regex: {e}", pattern.id)
                });
        }
    }

    #[test]
    fn snapshot_restorable_rules_make_no_recovery_claims() {
        let set = PatternSet::load().expect("patterns.toml must load");
        let mut copy: Vec<(&str, String)> = Vec::new();
        for p in set.patterns().iter().filter(|p| p.id == "FS-001") {
            let text = format!(
                "{} {}",
                p.description,
                p.justification.as_deref().unwrap_or_default()
            );
            copy.push((p.id.as_ref(), text.to_lowercase()));
        }
        for r in set
            .prefix_rules()
            .iter()
            .filter(|r| matches!(r.id.as_ref(), "FS-012" | "GIT-001" | "GIT-007"))
        {
            let text = format!(
                "{} {}",
                r.description,
                r.justification.as_deref().unwrap_or_default()
            );
            copy.push((r.id.as_ref(), text.to_lowercase()));
        }
        assert_eq!(
            copy.len(),
            4,
            "FS-001, FS-012, GIT-001 and GIT-007 must all exist"
        );
        for (id, text) in &copy {
            for banned in ["no undo", "no recovery", "irreversible"] {
                assert!(
                    !text.contains(banned),
                    "{id} copy claims {banned:?}: {text}"
                );
            }
        }
    }

    #[test]
    fn prefix_rule_rejects_a_blank_suppressing_token() {
        let ids = HashSet::new();
        let mut rule = rule_with_first(PatternToken::Single(Cow::Borrowed("npm")));

        rule.suppressed_by = &["--dry-run"];
        assert!(PatternSet::validate_prefix_rule(&rule, &ids).is_ok());

        rule.suppressed_by = &["--dry-run", "  "];
        assert!(PatternSet::validate_prefix_rule(&rule, &ids).is_err());
    }

    #[test]
    fn prefix_rule_rejects_an_empty_pattern() {
        let mut rule = rule_with_first(PatternToken::Single(Cow::Borrowed("npm")));
        rule.pattern.clear();

        match PatternSet::validate_prefix_rule(&rule, &HashSet::new()) {
            Err(ScannerError::InvalidPattern { id, reason }) => {
                assert_eq!(id, "T-001");
                assert_eq!(reason, "empty prefix rule pattern");
            }
            other => panic!("expected InvalidPattern, got {other:?}"),
        }
    }

    fn rule_with_first(first: PatternToken) -> PrefixRule {
        PrefixRule {
            id: Cow::Borrowed("T-001"),
            category: Category::Process,
            pattern: vec![first, PatternToken::AnyStar],
            risk: RiskLevel::Danger,
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: None,
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &[],
            not_match_examples: &[],
        }
    }

    #[test]
    fn prefix_rule_first_token_must_name_a_program() {
        let ids = HashSet::new();

        assert!(
            PatternSet::validate_prefix_rule(
                &rule_with_first(PatternToken::Single(Cow::Borrowed("rm"))),
                &ids
            )
            .is_ok()
        );
        assert!(
            PatternSet::validate_prefix_rule(
                &rule_with_first(PatternToken::Alts(vec![Cow::Borrowed("rm")])),
                &ids
            )
            .is_ok()
        );

        for first in [
            PatternToken::Any,
            PatternToken::AnyStar,
            PatternToken::ShortFlag {
                short: 'R',
                long: &["--recursive"],
            },
        ] {
            assert!(
                PatternSet::validate_prefix_rule(&rule_with_first(first.clone()), &ids).is_err(),
                "prefix rule with first token {first:?} must be rejected"
            );
        }
    }
}
