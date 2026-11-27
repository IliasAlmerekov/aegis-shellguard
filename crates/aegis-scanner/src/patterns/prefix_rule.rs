use std::borrow::Cow;
use std::sync::Arc;

use crate::patterns::PrefixRule;
use aegis_parser::EndOfOptions;
use aegis_types::MatchResult;
use aegis_types::{DetectionSource, MatchEvidence, Pattern};

impl PrefixRule {
    #[cfg(test)]
    pub fn matches_tokens(&self, tokens: &[&str]) -> bool {
        self.matches_tokens_with(tokens, EndOfOptions::Ignore)
    }

    pub(crate) fn matches_tokens_with(&self, tokens: &[&str], mode: EndOfOptions) -> bool {
        if aegis_parser::contains_any_token(tokens, self.suppressed_by) {
            return false;
        }

        if self.id.as_ref() == "FS-011" && wipefs_all_flag_present(tokens) {
            return true;
        }

        if self.id.as_ref() == "FS-020" {
            return rm_recursive_flag_present(tokens);
        }

        if self.id.as_ref() == "GIT-009" {
            return aegis_parser::matches_prefix_with(&self.pattern, tokens, mode)
                && git_push_deletes_remote_refs(tokens);
        }

        if self.id.as_ref() == "PS-008" {
            return rm_recursive_flag_present(tokens) && rm_root_operand_present(tokens);
        }

        if self.id.as_ref() == "DB-006"
            && tokens
                .first()
                .is_some_and(|t| t.eq_ignore_ascii_case("redis-cli"))
        {
            return redis_cli_flush_is_command(tokens);
        }

        aegis_parser::matches_prefix_with(&self.pattern, tokens, mode)
    }

    pub fn to_match_result(&self, tokens: &[&str]) -> MatchResult {
        let consumed = tokens.join(" ");
        MatchResult {
            pattern: Arc::new(Pattern {
                id: self.id.clone(),
                category: self.category,
                risk: self.risk,
                pattern: Cow::Owned(
                    self.pattern
                        .iter()
                        .map(|t| format!("{t:?}"))
                        .collect::<Vec<_>>()
                        .join(" "),
                ),
                description: self.description.clone(),
                safe_alt: self.safe_alt.clone(),
                justification: self.justification.clone(),
                source: self.source,
            }),
            matched_text: consumed,
            highlight_range: None,
            evidence: MatchEvidence::TokenPrefixRule {
                source: DetectionSource::from(self.source),
            },
        }
    }

    #[cfg(any(debug_assertions, test))]
    pub(crate) fn validate_examples(&self) -> Result<(), String> {
        for example in self.match_examples {
            let tokens = aegis_parser::split_tokens(example);
            let token_refs: Vec<&str> = tokens.iter().map(|s| s.as_str()).collect();
            if !self.matches_tokens_with(&token_refs, EndOfOptions::Honor) {
                return Err(format!(
                    "match_example {:?} does not match pattern {:?}",
                    example, self.pattern
                ));
            }
        }
        for example in self.not_match_examples {
            let tokens = aegis_parser::split_tokens(example);
            let token_refs: Vec<&str> = tokens.iter().map(|s| s.as_str()).collect();
            if self.matches_tokens_with(&token_refs, EndOfOptions::Honor) {
                return Err(format!(
                    "not_match_example {:?} unexpectedly matches pattern {:?}",
                    example, self.pattern
                ));
            }
        }
        Ok(())
    }
}

fn wipefs_all_flag_present(tokens: &[&str]) -> bool {
    if !tokens
        .first()
        .is_some_and(|program| program.eq_ignore_ascii_case("wipefs"))
    {
        return false;
    }

    tokens.iter().skip(1).any(|token| {
        if *token == "--all" {
            return true;
        }

        token
            .strip_prefix('-')
            .is_some_and(|short_flags| !short_flags.starts_with('-') && short_flags.contains('a'))
    })
}

fn rm_recursive_flag_present(tokens: &[&str]) -> bool {
    if !tokens
        .first()
        .is_some_and(|program| program.eq_ignore_ascii_case("rm"))
    {
        return false;
    }

    tokens
        .iter()
        .skip(1)
        .take_while(|token| **token != "--")
        .any(|token| is_rm_recursive_flag(token))
}

fn rm_root_operand_present(tokens: &[&str]) -> bool {
    tokens.iter().skip(1).any(|&token| is_root_path(token))
}

fn is_root_path(path: &str) -> bool {
    path.starts_with('/')
        && path
            .split('/')
            .all(|component| matches!(component, "" | "." | ".."))
}

fn is_rm_recursive_flag(token: &str) -> bool {
    if let Some(long) = token.strip_prefix("--") {
        return !long.is_empty() && "recursive".starts_with(long);
    }

    token
        .strip_prefix('-')
        .is_some_and(|short_flags| short_flags.contains('r') || short_flags.contains('R'))
}

fn git_push_deletes_remote_refs(tokens: &[&str]) -> bool {
    let mut deletes = false;
    let mut dry_run = false;
    let mut args = tokens.iter().skip(2);

    while let Some(&token) = args.next() {
        if let Some(long) = token.strip_prefix("--") {
            if GIT_PUSH_OPTIONS_WITH_VALUE.contains(&long) {
                args.next();
            } else if long == "dry-run" {
                dry_run = true;
            } else if is_git_long_option(long, "no-dry-run", 5) {
                dry_run = false;
            } else if is_git_long_option(long, "delete", 2)
                || is_git_long_option(long, "prune", 3)
                || is_git_long_option(long, "mirror", 1)
            {
                deletes = true;
            }
        } else if let Some(short_flags) = token.strip_prefix('-') {
            let (flags, push_option) = match short_flags.split_once('o') {
                Some((flags, value)) => (flags, Some(value)),
                None => (short_flags, None),
            };
            deletes |= flags.contains('d');
            dry_run |= flags.contains('n');
            if push_option == Some("") {
                args.next();
            }
        } else {
            let refspec = token.strip_prefix('+').unwrap_or(token);
            deletes |= refspec.len() > 1 && refspec.starts_with(':');
        }
    }

    deletes && !dry_run
}

const GIT_PUSH_OPTIONS_WITH_VALUE: &[&str] = &[
    "exec",
    "push-option",
    "receive-pack",
    "recurse-submodules",
    "repo",
];

fn is_git_long_option(given: &str, full: &str, min_len: usize) -> bool {
    given.len() >= min_len && full.starts_with(given)
}

const REDIS_CLI_FLAGS_WITH_VALUE: &[&str] = &[
    "-h",
    "-p",
    "-a",
    "-n",
    "-u",
    "-s",
    "-d",
    "-i",
    "-r",
    "--user",
    "--pass",
    "--sni",
    "--cacert",
    "--cacertdir",
    "--cert",
    "--key",
    "--pipe-timeout",
    "--connect-timeout",
    "--cluster",
];

fn redis_cli_flush_is_command(tokens: &[&str]) -> bool {
    debug_assert!(
        tokens
            .first()
            .is_some_and(|t| t.eq_ignore_ascii_case("redis-cli")),
        "redis_cli_flush_is_command called on non-redis-cli tokens"
    );

    let mut i = 1;
    while i < tokens.len() {
        let token = tokens[i];
        if token.starts_with('-') {
            i += 1;
            if REDIS_CLI_FLAGS_WITH_VALUE.contains(&token) {
                i += 1;
            }
        } else {
            return token.eq_ignore_ascii_case("FLUSHALL") || token.eq_ignore_ascii_case("FLUSHDB");
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patterns::PrefixRule;
    use aegis_types::{Category, PatternSource, PatternToken, RiskLevel};

    fn single(s: &'static str) -> PatternToken {
        PatternToken::Single(Cow::Borrowed(s))
    }

    fn alts(alts: &[&'static str]) -> PatternToken {
        PatternToken::Alts(alts.iter().map(|&s| Cow::Borrowed(s)).collect())
    }

    #[test]
    fn prefix_rule_matches_single_token() {
        let rule = PrefixRule {
            id: Cow::Borrowed("T-001"),
            category: Category::Process,
            pattern: vec![single("rm")],
            risk: RiskLevel::Danger,
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: None,
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &[],
            not_match_examples: &[],
        };
        assert!(rule.matches_tokens(&["rm", "-rf", "/"]));
    }

    #[test]
    fn prefix_rule_matches_multiple_tokens() {
        let rule = PrefixRule {
            id: Cow::Borrowed("T-002"),
            category: Category::Git,
            pattern: vec![single("git"), single("push")],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: None,
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &[],
            not_match_examples: &[],
        };
        assert!(rule.matches_tokens(&["git", "push", "origin", "main"]));
    }

    #[test]
    fn prefix_rule_matches_with_alts() {
        let rule = PrefixRule {
            id: Cow::Borrowed("T-003"),
            category: Category::Git,
            pattern: vec![single("git"), single("push"), alts(&["--force", "-f"])],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: None,
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &[],
            not_match_examples: &[],
        };
        assert!(rule.matches_tokens(&["git", "push", "--force"]));
        assert!(rule.matches_tokens(&["git", "push", "-f"]));
    }

    #[test]
    fn prefix_rule_fails_on_token_mismatch() {
        let rule = PrefixRule {
            id: Cow::Borrowed("T-004"),
            category: Category::Git,
            pattern: vec![single("git"), single("push")],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: None,
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &[],
            not_match_examples: &[],
        };
        assert!(!rule.matches_tokens(&["git", "status"]));
    }

    #[test]
    fn prefix_rule_fails_on_insufficient_tokens() {
        let rule = PrefixRule {
            id: Cow::Borrowed("T-005"),
            category: Category::Git,
            pattern: vec![single("git"), single("push"), single("origin")],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: None,
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &[],
            not_match_examples: &[],
        };
        assert!(!rule.matches_tokens(&["git", "push"]));
    }

    #[test]
    fn prefix_rule_alts_fails_when_none_match() {
        let rule = PrefixRule {
            id: Cow::Borrowed("T-006"),
            category: Category::Git,
            pattern: vec![single("git"), single("push"), alts(&["--force", "-f"])],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: None,
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &[],
            not_match_examples: &[],
        };
        assert!(!rule.matches_tokens(&["git", "push", "--dry-run"]));
    }

    #[test]
    fn prefix_rule_exact_length_match() {
        let rule = PrefixRule {
            id: Cow::Borrowed("T-007"),
            category: Category::Filesystem,
            pattern: vec![single("rm"), single("-rf")],
            risk: RiskLevel::Danger,
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: None,
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &[],
            not_match_examples: &[],
        };
        assert!(rule.matches_tokens(&["rm", "-rf"]));
    }

    #[test]
    fn prefix_rule_empty_pattern_never_matches() {
        let rule = PrefixRule {
            id: Cow::Borrowed("T-008"),
            category: Category::Process,
            pattern: vec![],
            risk: RiskLevel::Safe,
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: None,
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &[],
            not_match_examples: &[],
        };
        assert!(!rule.matches_tokens(&["anything", "here"]));
        assert!(!rule.matches_tokens(&[]));
    }

    #[test]
    fn prefix_rule_case_insensitive_for_commands() {
        let rule = PrefixRule {
            id: Cow::Borrowed("T-009"),
            category: Category::Git,
            pattern: vec![single("Git")],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: None,
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &[],
            not_match_examples: &[],
        };
        assert!(rule.matches_tokens(&["git"]));
        assert!(rule.matches_tokens(&["Git"]));
    }

    #[test]
    fn prefix_rule_case_sensitive_for_flags() {
        let rule = PrefixRule {
            id: Cow::Borrowed("T-013"),
            category: Category::Git,
            pattern: vec![single("git"), single("branch"), single("-D")],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: None,
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &[],
            not_match_examples: &[],
        };
        assert!(rule.matches_tokens(&["git", "branch", "-D"]));
        assert!(!rule.matches_tokens(&["git", "branch", "-d"]));
    }

    #[test]
    fn prefix_rule_multiple_alts_positions() {
        let rule = PrefixRule {
            id: Cow::Borrowed("T-010"),
            category: Category::Cloud,
            pattern: vec![
                single("aws"),
                alts(&["ec2", "s3"]),
                alts(&["delete", "terminate"]),
            ],
            risk: RiskLevel::Danger,
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: None,
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &[],
            not_match_examples: &[],
        };
        assert!(rule.matches_tokens(&["aws", "ec2", "delete"]));
        assert!(rule.matches_tokens(&["aws", "s3", "terminate"]));
        assert!(!rule.matches_tokens(&["aws", "ec2", "create"]));
    }

    #[test]
    fn prefix_rule_any_star_matches_zero_tokens() {
        let rule = PrefixRule {
            id: Cow::Borrowed("T-011"),
            category: Category::Git,
            pattern: vec![single("git"), PatternToken::AnyStar, single("status")],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: None,
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &[],
            not_match_examples: &[],
        };
        assert!(rule.matches_tokens(&["git", "status"]));
        assert!(rule.matches_tokens(&["git", "log", "status"]));
        assert!(rule.matches_tokens(&["git", "a", "b", "c", "status"]));
        assert!(!rule.matches_tokens(&["git", "log"]));
    }

    #[test]
    fn prefix_rule_any_matches_one_token() {
        let rule = PrefixRule {
            id: Cow::Borrowed("T-012"),
            category: Category::Git,
            pattern: vec![single("git"), PatternToken::Any, single("status")],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: None,
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &[],
            not_match_examples: &[],
        };
        assert!(rule.matches_tokens(&["git", "log", "status"]));
        assert!(!rule.matches_tokens(&["git", "status"]));
        assert!(!rule.matches_tokens(&["git", "a", "b", "status"]));
    }

    #[test]
    fn prefix_rule_to_match_result_copies_justification() {
        let rule = PrefixRule {
            id: Cow::Borrowed("T-013"),
            category: Category::Git,
            pattern: vec![single("git"), single("push"), single("--force")],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: Some(Cow::Borrowed("rewrites remote history")),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &[],
            not_match_examples: &[],
        };
        let result = rule.to_match_result(&["git", "push", "--force"]);
        assert_eq!(
            result.pattern.justification.as_deref(),
            Some("rewrites remote history")
        );
    }

    #[test]
    fn prefix_rule_validate_examples_detects_bad_match_example() {
        let rule = PrefixRule {
            id: Cow::Borrowed("BAD-001"),
            category: Category::Process,
            pattern: vec![single("rm")],
            risk: RiskLevel::Danger,
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: Some(Cow::Borrowed("test")),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &["echo hello"],
            not_match_examples: &[],
        };
        assert!(
            rule.validate_examples().is_err(),
            "validate_examples must reject a rule whose match_examples do not actually match"
        );
    }

    #[test]
    fn fs011_matches_wipefs_short_flag_bundle_containing_all_flag() {
        let rule = PrefixRule {
            id: Cow::Borrowed("FS-011"),
            category: Category::Filesystem,
            pattern: vec![
                single("wipefs"),
                PatternToken::AnyStar,
                alts(&["-a", "--all"]),
            ],
            risk: RiskLevel::Danger,
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: None,
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &[],
            not_match_examples: &[],
        };

        assert!(rule.matches_tokens(&["wipefs", "-af", "/dev/sda"]));
        assert!(rule.matches_tokens(&["wipefs", "-fa", "/dev/sda"]));
        assert!(rule.matches_tokens(&["wipefs", "-fav", "/dev/sda"]));
        assert!(rule.matches_tokens(&["wipefs", "-av", "/dev/sda"]));
        assert!(rule.matches_tokens(&["wipefs", "--all", "/dev/sda"]));
        assert!(!rule.matches_tokens(&["wipefs", "-f", "/dev/sda"]));
        assert!(!rule.matches_tokens(&["wipefs", "-vn", "/dev/sda"]));
        assert!(!rule.matches_tokens(&["wipefs", "--almost", "/dev/sda"]));
    }

    fn rule_suppressed_by(suppressed_by: &'static [&'static str]) -> PrefixRule {
        PrefixRule {
            id: Cow::Borrowed("T-014"),
            category: Category::Process,
            pattern: vec![single("npm"), single("publish")],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: None,
            source: PatternSource::Builtin,
            suppressed_by,
            match_examples: &[],
            not_match_examples: &[],
        }
    }

    #[test]
    fn prefix_rule_without_suppressing_tokens_is_unchanged() {
        let rule = rule_suppressed_by(&[]);
        assert!(rule.matches_tokens(&["npm", "publish"]));
        assert!(rule.matches_tokens(&["npm", "publish", "--dry-run"]));
    }

    #[test]
    fn prefix_rule_does_not_fire_when_suppressing_token_present() {
        let rule = rule_suppressed_by(&["--dry-run"]);
        assert!(rule.matches_tokens(&["npm", "publish"]));
        assert!(!rule.matches_tokens(&["npm", "publish", "--dry-run"]));
    }

    #[test]
    fn prefix_rule_suppression_is_position_independent() {
        let rule = rule_suppressed_by(&["--dry-run"]);
        assert!(!rule.matches_tokens(&["npm", "publish", "--dry-run"]));
        assert!(!rule.matches_tokens(&["npm", "publish", "--access", "public", "--dry-run"]));
        assert!(!rule.matches_tokens(&["npm", "--dry-run", "publish"]));
    }

    #[test]
    fn prefix_rule_any_listed_suppressing_token_silences_the_rule() {
        let rule = rule_suppressed_by(&["--dry-run", "-n"]);
        assert!(!rule.matches_tokens(&["npm", "publish", "-n"]));
        assert!(!rule.matches_tokens(&["npm", "publish", "--dry-run"]));
        assert!(rule.matches_tokens(&["npm", "publish", "--access", "public"]));
    }

    #[test]
    fn prefix_rule_suppression_requires_a_whole_token() {
        let rule = rule_suppressed_by(&["--dry-run"]);
        assert!(rule.matches_tokens(&["npm", "publish", "--dry-run-later"]));
    }

    #[test]
    fn prefix_rule_suppression_outranks_a_matcher_side_predicate() {
        let rule = PrefixRule {
            id: Cow::Borrowed("FS-011"),
            category: Category::Filesystem,
            pattern: vec![
                single("wipefs"),
                PatternToken::AnyStar,
                alts(&["-a", "--all"]),
            ],
            risk: RiskLevel::Danger,
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: None,
            source: PatternSource::Builtin,
            suppressed_by: &["--no-act"],
            match_examples: &[],
            not_match_examples: &[],
        };
        assert!(rule.matches_tokens(&["wipefs", "-af", "/dev/sda"]));
        assert!(!rule.matches_tokens(&["wipefs", "-af", "--no-act", "/dev/sda"]));
    }

    #[test]
    fn prefix_rule_not_match_example_can_pin_the_negative_condition() {
        let rule = PrefixRule {
            id: Cow::Borrowed("T-015"),
            category: Category::Process,
            pattern: vec![single("npm"), single("publish")],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: None,
            source: PatternSource::Builtin,
            suppressed_by: &["--dry-run"],
            match_examples: &["npm publish"],
            not_match_examples: &["npm publish --dry-run"],
        };
        assert!(
            rule.validate_examples().is_ok(),
            "a not_match_example covered by the negative condition must validate"
        );
    }

    #[test]
    fn prefix_rule_example_validation_catches_a_dropped_negative_condition() {
        let rule = PrefixRule {
            id: Cow::Borrowed("T-016"),
            category: Category::Process,
            pattern: vec![single("npm"), single("publish")],
            risk: RiskLevel::Warn,
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: None,
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &["npm publish"],
            not_match_examples: &["npm publish --dry-run"],
        };
        assert!(rule.validate_examples().is_err());
    }

    #[test]
    fn prefix_rule_validate_examples_detects_bad_not_match_example() {
        let rule = PrefixRule {
            id: Cow::Borrowed("BAD-002"),
            category: Category::Process,
            pattern: vec![single("rm")],
            risk: RiskLevel::Danger,
            description: Cow::Borrowed("test"),
            safe_alt: None,
            justification: Some(Cow::Borrowed("test")),
            source: PatternSource::Builtin,
            suppressed_by: &[],
            match_examples: &[],
            not_match_examples: &["rm -rf /"],
        };
        assert!(
            rule.validate_examples().is_err(),
            "validate_examples must reject a rule whose not_match_examples actually match"
        );
    }
}
