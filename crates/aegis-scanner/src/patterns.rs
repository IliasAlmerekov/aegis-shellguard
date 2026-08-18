// Pattern struct, Category, loading

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::Arc;

use serde::Deserialize;

pub use aegis_types::{Category, Pattern, PatternSource, PatternToken, PrefixPattern};

use aegis_types::RiskLevel;

use crate::error::ScannerError;

// ── Token-prefix rule types (live alongside regex-based Pattern) ──────────

/// A token-level prefix rule that matches the beginning of a tokenized command.
///
/// Replaces free-form regex for commands whose dangerous semantics are fully
/// captured by a fixed prefix of tokens (e.g. `git push --force`).
#[derive(Debug, Clone)]
pub struct PrefixRule {
    /// Unique pattern identifier.
    pub id: Cow<'static, str>,
    /// Semantic category.
    pub category: Category,
    /// Token prefix to match against.
    pub pattern: PrefixPattern,
    /// Assigned risk level when this rule matches.
    pub risk: RiskLevel,
    /// Human-readable description of what this rule detects.
    pub description: Cow<'static, str>,
    /// Safer alternative to suggest, if any.
    pub safe_alt: Option<Cow<'static, str>>,
    /// Optional rationale for this rule.
    pub justification: Option<Cow<'static, str>>,
    /// Whether the rule is built-in or user-defined.
    pub source: PatternSource,
    /// Negative condition: tokens whose presence anywhere in the command's
    /// tokens suppresses this rule.
    ///
    /// A prefix rule matches a prefix and ignores the tail, so a rule on
    /// `npm publish` also fires on `npm publish --dry-run` — a safe rehearsal.
    /// Listing `--dry-run` here silences the rule for that form. Position is
    /// irrelevant: the token is looked for anywhere, because a flag can be
    /// written anywhere in the tail.
    ///
    /// Cost: a rule declaring this can be silenced by the presence of a token,
    /// so the list must stay short and semantically unambiguous — every token
    /// on it must mean "this invocation does not perform the action" on its
    /// own, regardless of what else is on the command line.
    ///
    /// An empty list (the default for every rule that does not need one) leaves
    /// matching exactly as it was.
    pub suppressed_by: &'static [&'static str],
    /// Example commands that MUST match this prefix rule.
    pub match_examples: &'static [&'static str],
    /// Example commands that MUST NOT match this prefix rule.
    pub not_match_examples: &'static [&'static str],
}

/// Internal helper: TOML-deserializable representation before conversion to [`Pattern`].
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

/// Wrapper for TOML top-level table: `[[patterns]]`.
#[derive(Debug, Deserialize)]
struct PatternsFile {
    patterns: Vec<RawPattern>,
}

/// Effective merged pattern set consumed when constructing a scanner.
///
/// This is the authoritative runtime view after combining the built-in
/// patterns embedded in the binary with any custom patterns supplied by the
/// resolved config layers.
#[derive(Debug)]
pub struct PatternSet {
    patterns: Vec<Arc<Pattern>>,
    prefix_rules: Vec<Arc<PrefixRule>>,
}

/// Built-in patterns embedded at compile time — binary stays self-contained.
const BUILTIN_PATTERNS_TOML: &str = include_str!("../patterns.toml");

impl PatternSet {
    /// Parse and return the canonical built-in-only pattern set.
    ///
    /// This loads the embedded `config/patterns.toml` without any config
    /// overlays, providing the built-in source of truth before custom patterns
    /// are merged for runtime scanner construction.
    pub fn load() -> Result<PatternSet, ScannerError> {
        Self::from_sources(&[])
    }

    /// Build the authoritative merged pattern view for scanner construction.
    ///
    /// Merge order is fixed and explicit:
    /// 1) built-in patterns embedded in the binary
    /// 2) custom patterns supplied by the caller
    ///
    /// Custom patterns arrive already normalized into the neutral [`Pattern`]
    /// representation — conversion from any config-specific shape happens at the
    /// orchestration boundary, so the scanner never sees config types. The
    /// returned set is the effective runtime input consumed by
    /// `Scanner::try_new`, after validation. Note: regex *syntactic* validity is
    /// not checked here (only field presence); it is enforced when the scanner
    /// compiles the patterns.
    pub fn from_sources(custom_patterns: &[Pattern]) -> Result<PatternSet, ScannerError> {
        let file: PatternsFile = toml::from_str(BUILTIN_PATTERNS_TOML)
            .map_err(|e| ScannerError::Build(format!("failed to parse patterns.toml: {e}")))?;

        // 1) built-in
        let builtin_patterns: Vec<Pattern> = file.patterns.into_iter().map(Pattern::from).collect();

        // 2) validate unified set (required fields + duplicate IDs forbidden for regex patterns).
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

        // 5) compile built-in prefix rules.
        let prefix_rules = builtin_prefix_rules();

        // 5a) validate built-in prefix rules against their examples in debug builds and tests.
        //    A rule that fails its own examples is a bug and must be fixed before the binary ships.
        #[cfg(debug_assertions)]
        for rule in &prefix_rules {
            if let Err(e) = rule.validate_examples() {
                panic!(
                    "built-in prefix rule {} failed example validation: {e}",
                    rule.id
                );
            }
        }

        // 6) validate prefix rules: required fields + no conflict with regex pattern IDs.
        //    Duplicate IDs within prefix rules are intentional: the same logical rule can
        //    have multiple syntactic forms (e.g. "docker-compose" vs "docker compose").
        for rule in &prefix_rules {
            Self::validate_prefix_rule(rule, &pattern_ids)?;
        }

        let prefix_rules: Vec<Arc<PrefixRule>> = prefix_rules.into_iter().map(Arc::new).collect();

        // 7) compiled into runtime PatternSet.
        Ok(PatternSet {
            patterns,
            prefix_rules,
        })
    }

    /// Return the effective merged regex pattern set consumed by scanner construction.
    pub fn patterns(&self) -> &[Arc<Pattern>] {
        self.patterns.as_slice()
    }

    /// Return the effective merged prefix-rule set consumed by scanner construction.
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

        if rule.pattern.is_empty() {
            return Err(ScannerError::InvalidPattern {
                id: rule.id.to_string(),
                reason: "empty prefix rule pattern".to_string(),
            });
        }

        // The first token must name a program (Single or Alts). A rule whose
        // first token is a wildcard or a flag cannot be indexed by an Effective
        // program (ADR-014) and would never fire at runtime — reject it instead
        // of silently dropping it from the index.
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
            None => unreachable!("empty prefix rule pattern rejected above"),
        }

        if rule.description.trim().is_empty() {
            return Err(ScannerError::InvalidPattern {
                id: rule.id.to_string(),
                reason: "empty prefix rule description".to_string(),
            });
        }

        // A blank suppressing token can never equal a real token, so it is dead
        // data that reads as an active negative condition — reject it rather
        // than let a rule look guarded when it is not.
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

        // Prevent a prefix rule from shadowing a regex pattern with the same ID.
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

// ── Built-in prefix rules (replaces regex for token-prefixable commands) ───

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
    use std::collections::HashSet;

    #[test]
    fn prefix_rule_rejects_a_blank_suppressing_token() {
        let ids = HashSet::new();
        let mut rule = rule_with_first(PatternToken::Single(Cow::Borrowed("npm")));

        rule.suppressed_by = &["--dry-run"];
        assert!(PatternSet::validate_prefix_rule(&rule, &ids).is_ok());

        // A blank entry can never equal a real token: it reads as a guard that
        // is not there.
        rule.suppressed_by = &["--dry-run", "  "];
        assert!(PatternSet::validate_prefix_rule(&rule, &ids).is_err());
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

        // Valid: Single and Alts name a program and are accepted.
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

        // Invalid: a wildcard or flag first token cannot be indexed by an
        // Effective program and would never fire at runtime — reject it.
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
