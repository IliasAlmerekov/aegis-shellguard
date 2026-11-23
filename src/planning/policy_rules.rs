//! Runtime evaluation of typed `[[rules]]` entries against a raw command string.

use aegis_config::{PolicyRule, policy_pattern_matches};
use aegis_parser::{canonical_ansi_c_quoting, logical_segments, split_tokens};
use aegis_policy::PolicyRulesResult;
use aegis_types::PolicyRuleDecision;

/// Evaluate `[[rules]]` entries against a raw command string.
///
/// - Tokenizes the first logical segment with the quote-aware parser.
/// - If the command is compound (more than one logical segment) and the
///   matching rule's effective decision is `Allow`, downgrades to `Prompt` so
///   the user reviews the full command chain.
/// - Returns the default (not matched) when no rule matches.
pub fn evaluate_policy_rules(rules: &[PolicyRule], cmd: &str) -> PolicyRulesResult {
    // Segment boundaries come from the words bash runs; see
    // `canonical_ansi_c_quoting`.
    let cmd = canonical_ansi_c_quoting(cmd);
    let segments = logical_segments(&cmd);
    let first_segment: &str = segments.first().map(String::as_str).unwrap_or(&cmd);
    let is_compound = segments.len() > 1;

    let token_strings: Vec<String> = split_tokens(first_segment);
    let tokens: Vec<&str> = token_strings.iter().map(String::as_str).collect();

    for rule in rules {
        if policy_pattern_matches(&rule.pattern, &tokens) {
            let decision = effective_decision(rule, is_compound);
            return PolicyRulesResult {
                matched: true,
                decision: Some(decision),
                justification: rule
                    .justification
                    .as_deref()
                    .map(|s| std::borrow::Cow::Owned(s.to_owned())),
            };
        }
    }
    PolicyRulesResult::default()
}

/// Resolve the final decision for a matched rule, applying `when`-clause and
/// compound-command downgrade.
///
/// Compound commands are never silently approved — a rule that would `allow`
/// a simple command can at most `prompt` when additional shell segments follow.
fn effective_decision(rule: &PolicyRule, is_compound: bool) -> PolicyRuleDecision {
    let base = if let Some(ref when) = rule.when {
        // Missing or non-Unicode env var → condition not met.
        let env_matches = std::env::var(&when.env)
            .map(|val| val == when.value)
            .unwrap_or(false);
        if env_matches {
            when.then
        } else {
            rule.decision
        }
    } else {
        rule.decision
    };

    if is_compound && base == PolicyRuleDecision::Allow {
        PolicyRuleDecision::Prompt
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use aegis_config::{AegisConfig, PolicyPatternToken, PolicyRule};
    use aegis_parser::Parser as CommandParser;
    use aegis_policy::{
        ExecutionTransport, PolicyAction, PolicyAllowlistResult, PolicyBlocklistResult,
        PolicyCiState, PolicyConfigFlags, PolicyDecision, PolicyExecutionContext, PolicyInput,
        PolicyRationale, evaluate_policy,
    };
    use aegis_types::Assessment;
    use aegis_types::PolicyRuleDecision;
    use aegis_types::{AllowlistOverrideLevel, CiPolicy, Mode, RiskLevel, SnapshotPolicy};
    use std::fs;
    use tempfile::TempDir;

    use super::evaluate_policy_rules;

    fn single(s: &str) -> PolicyPatternToken {
        PolicyPatternToken::Single(s.to_string())
    }

    fn alts(v: &[&str]) -> PolicyPatternToken {
        PolicyPatternToken::Alts(v.iter().map(|s| s.to_string()).collect())
    }

    fn rule_with_pattern(
        pattern: Vec<PolicyPatternToken>,
        decision: PolicyRuleDecision,
    ) -> PolicyRule {
        PolicyRule {
            pattern,
            decision,
            justification: None,
            match_examples: vec![],
            not_match_examples: vec![],
            when: None,
        }
    }

    #[test]
    fn evaluate_policy_rules_matches_single_token_pattern() {
        let rules = vec![rule_with_pattern(
            vec![single("git"), single("push")],
            PolicyRuleDecision::Prompt,
        )];
        let result = evaluate_policy_rules(&rules, "git push origin main");
        assert!(result.matched);
        assert_eq!(result.decision, Some(PolicyRuleDecision::Prompt));
    }

    #[test]
    fn evaluate_policy_rules_alts_match() {
        let rules = vec![rule_with_pattern(
            vec![single("git"), single("push"), alts(&["--force", "-f"])],
            PolicyRuleDecision::Block,
        )];
        let result = evaluate_policy_rules(&rules, "git push -f origin");
        assert!(result.matched);
        assert_eq!(result.decision, Some(PolicyRuleDecision::Block));
    }

    #[test]
    fn evaluate_policy_rules_no_match_returns_default() {
        let rules = vec![rule_with_pattern(
            vec![single("git"), single("push")],
            PolicyRuleDecision::Prompt,
        )];
        let result = evaluate_policy_rules(&rules, "echo hello");
        assert!(!result.matched);
        assert_eq!(result.decision, None);
    }

    #[test]
    fn compound_command_allow_rule_downgrades_to_prompt() {
        let rules = vec![rule_with_pattern(
            vec![single("git"), single("status")],
            PolicyRuleDecision::Allow,
        )];
        // Compound command — the git status part matches but tail is dangerous.
        let result = evaluate_policy_rules(&rules, "git status && terraform destroy");
        assert!(result.matched);
        // Allow must be downgraded to Prompt for compound commands.
        assert_eq!(result.decision, Some(PolicyRuleDecision::Prompt));
    }

    #[test]
    fn ansi_c_quoted_escaped_quote_still_counts_as_compound() {
        let rules = vec![rule_with_pattern(
            vec![single("echo")],
            PolicyRuleDecision::Allow,
        )];
        let result = evaluate_policy_rules(&rules, r"echo $'\'' ; terraform destroy ; echo $'\''");
        assert!(result.matched);
        assert_eq!(result.decision, Some(PolicyRuleDecision::Prompt));
    }

    #[test]
    fn ansi_c_quoted_program_word_matches_the_decoded_rule() {
        let rules = vec![rule_with_pattern(
            vec![single("terraform"), single("destroy")],
            PolicyRuleDecision::Block,
        )];
        let result = evaluate_policy_rules(&rules, r"$'\x74erraform' destroy");
        assert_eq!(result.decision, Some(PolicyRuleDecision::Block));
    }

    #[test]
    fn compound_command_block_rule_stays_block() {
        let rules = vec![rule_with_pattern(
            vec![single("git")],
            PolicyRuleDecision::Block,
        )];
        let result = evaluate_policy_rules(&rules, "git status && terraform destroy");
        assert!(result.matched);
        assert_eq!(result.decision, Some(PolicyRuleDecision::Block));
    }

    #[test]
    fn quoted_args_tokenized_correctly() {
        let rules = vec![rule_with_pattern(
            vec![single("git"), single("commit")],
            PolicyRuleDecision::Prompt,
        )];
        // The quoted message should be handled as one token by the quote-aware parser.
        let result = evaluate_policy_rules(&rules, r#"git commit -m "feat: add feature""#);
        assert!(result.matched);
    }

    // Project `[[rules]]` Allow is dropped at merge so Danger commands still prompt under Protect (ADR-013).

    fn danger_assessment(cmd: &str) -> Assessment {
        Assessment {
            risk: RiskLevel::Danger,
            effect_opaque: false,
            matched: Vec::new(),
            highlight_ranges: Vec::new(),
            command: CommandParser::parse(cmd),
            analysis: None,
        }
    }

    #[test]
    fn project_rules_allow_does_not_autoapprove_danger_under_protect() {
        let workspace = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let global_dir = home.path().join(".config/aegis");
        fs::create_dir_all(&global_dir).unwrap();

        // Base (global): Protect mode, no rules.
        fs::write(global_dir.join("config.toml"), "mode = \"Protect\"\n").unwrap();
        // Project layer attempts to auto-approve `terraform ...` via a rule.
        fs::write(
            workspace.path().join(".aegis.toml"),
            "[[rules]]\npattern = [\"terraform\"]\ndecision = \"allow\"\n",
        )
        .unwrap();

        let config =
            AegisConfig::load_for(workspace.path(), Some(home.path())).expect("config must load");

        // The project Allow rule must have been dropped at merge — it must NOT
        // match the danger command.
        let rules_result =
            evaluate_policy_rules(&config.rules, "terraform destroy -target=module.prod.api");
        assert!(
            !rules_result.matched,
            "project-layer [[rules]] Allow must be dropped so it does not match; \
             got rules_result = {:?} (merged rules = {:?})",
            rules_result, config.rules,
        );

        // And consequently the engine must Prompt (NOT auto-approve) for the
        // Danger command under Mode::Protect.
        let assessment = danger_assessment("terraform destroy -target=module.prod.api");
        let decision: PolicyDecision = evaluate_policy(PolicyInput {
            assessment: &assessment,
            mode: Mode::Protect,
            ci_state: PolicyCiState { detected: false },
            allowlist: PolicyAllowlistResult { matched: false },
            blocklist: PolicyBlocklistResult { matched: false },
            config_flags: PolicyConfigFlags {
                ci_policy: CiPolicy::Allow,
                allowlist_override_level: AllowlistOverrideLevel::Never,
                snapshot_policy: SnapshotPolicy::None,
            },
            execution_context: PolicyExecutionContext {
                transport: ExecutionTransport::Shell,
                applicable_snapshot_plugins: &[],
            },
            rules: rules_result,
        });

        assert_eq!(
            decision.decision,
            PolicyAction::Prompt,
            "a dropped project Allow must leave a Danger command prompting under Protect; \
             got decision = {:?}",
            decision,
        );
        assert_ne!(
            decision.decision,
            PolicyAction::AutoApprove,
            "project [[rules]] Allow must NOT auto-approve a Danger command; got decision = {:?}",
            decision,
        );
        assert_eq!(decision.rationale, PolicyRationale::RequiresConfirmation);
    }

    #[test]
    fn global_rules_allow_still_autoapproves_under_protect() {
        let workspace = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let global_dir = home.path().join(".config/aegis");
        fs::create_dir_all(&global_dir).unwrap();

        fs::write(
            global_dir.join("config.toml"),
            "[[rules]]\npattern = [\"terraform\"]\ndecision = \"allow\"\n",
        )
        .unwrap();
        // No project file — only the global layer is in play.

        let config =
            AegisConfig::load_for(workspace.path(), Some(home.path())).expect("config must load");

        assert_eq!(
            config.rules.len(),
            1,
            "global [[rules]] Allow must be present in merged rules; got {:?}",
            config.rules,
        );
        assert_eq!(config.rules[0].decision, PolicyRuleDecision::Allow);

        let rules_result =
            evaluate_policy_rules(&config.rules, "terraform destroy -target=module.prod.api");
        assert!(
            rules_result.matched,
            "global [[rules]] Allow must still match; got {:?}",
            rules_result,
        );
        assert_eq!(rules_result.decision, Some(PolicyRuleDecision::Allow));

        let assessment = danger_assessment("terraform destroy -target=module.prod.api");
        let decision: PolicyDecision = evaluate_policy(PolicyInput {
            assessment: &assessment,
            mode: Mode::Protect,
            ci_state: PolicyCiState { detected: false },
            allowlist: PolicyAllowlistResult { matched: false },
            blocklist: PolicyBlocklistResult { matched: false },
            config_flags: PolicyConfigFlags {
                ci_policy: CiPolicy::Allow,
                allowlist_override_level: AllowlistOverrideLevel::Never,
                snapshot_policy: SnapshotPolicy::None,
            },
            execution_context: PolicyExecutionContext {
                transport: ExecutionTransport::Shell,
                applicable_snapshot_plugins: &[],
            },
            rules: rules_result,
        });

        assert_eq!(
            decision.decision,
            PolicyAction::AutoApprove,
            "global [[rules]] Allow must still auto-approve; got decision = {:?}",
            decision,
        );
        assert_eq!(decision.rationale, PolicyRationale::PolicyRulesOverride);
    }
}
