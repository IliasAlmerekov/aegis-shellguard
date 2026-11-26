#![expect(clippy::unwrap_used, reason = "test helpers fail loudly")]
use std::fs;
#[path = "support/isolated_test.rs"]
mod isolated_test;

use aegis::planning::evaluate_policy_rules;
use aegis_config::{AegisConfig, PolicyPatternToken, PolicyRule, WhenClause};
use aegis_parser::Parser as CommandParser;
use aegis_policy::{
    ExecutionTransport, PolicyAction, PolicyAllowlistResult, PolicyBlocklistResult, PolicyCiState,
    PolicyConfigFlags, PolicyDecision, PolicyExecutionContext, PolicyInput, evaluate_policy,
};
use aegis_types::{
    AllowlistOverrideLevel, Assessment, CiPolicy, Mode, PolicyRuleDecision, RiskLevel,
    SnapshotPolicy,
};
use tempfile::TempDir;

fn in_child(key: &str, value: Option<&str>) -> Option<isolated_test::Completion> {
    const CHILD: &str = "AEGIS_POLICY_ENV_CHILD";
    if std::env::var_os(CHILD).is_some() {
        return Some(isolated_test::Completion);
    }
    let mut command = isolated_test::command();
    command.env(CHILD, "1");
    match value {
        Some(value) => {
            command.env(key, value);
        }
        None => {
            command.env_remove(key);
        }
    }
    isolated_test::run(&mut command);
    None
}

#[test]
fn unknown_isolated_test_is_not_a_success() {
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", "no_such_test", "--nocapture"]);
    let output = command.output().unwrap();
    assert!(output.status.success());
    assert!(!isolated_test::completed(&output));
}

fn single(s: &str) -> PolicyPatternToken {
    PolicyPatternToken::Single(s.to_string())
}

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
fn evaluate_policy_rules_when_clause_overrides_on_env_match() {
    if let Some(_completion) = in_child("AEGIS_TEST_ENV_VAR_UNIQUE", Some("true")) {
        let rules = vec![PolicyRule {
            pattern: vec![single("rm")],
            decision: PolicyRuleDecision::Prompt,
            justification: None,
            match_examples: vec![],
            not_match_examples: vec![],
            when: Some(WhenClause {
                env: "AEGIS_TEST_ENV_VAR_UNIQUE".to_string(),
                value: "true".to_string(),
                then: PolicyRuleDecision::Allow,
            }),
        }];
        let result = evaluate_policy_rules(&rules, "rm -rf build");
        assert!(result.matched);
        assert_eq!(result.decision, Some(PolicyRuleDecision::Allow));
    }
}

#[test]
fn evaluate_policy_rules_when_clause_uses_base_on_env_mismatch() {
    if let Some(_completion) = in_child("AEGIS_TEST_ENV_VAR_ABSENT", None) {
        let rules = vec![PolicyRule {
            pattern: vec![single("rm")],
            decision: PolicyRuleDecision::Prompt,
            justification: None,
            match_examples: vec![],
            not_match_examples: vec![],
            when: Some(WhenClause {
                env: "AEGIS_TEST_ENV_VAR_ABSENT".to_string(),
                value: "true".to_string(),
                then: PolicyRuleDecision::Allow,
            }),
        }];
        let result = evaluate_policy_rules(&rules, "rm -rf build");
        assert!(result.matched);
        assert_eq!(result.decision, Some(PolicyRuleDecision::Prompt));
    }
}

#[test]
fn evaluate_policy_rules_when_missing_env_empty_value_does_not_match() {
    if let Some(_completion) = in_child("AEGIS_TEST_ABSENT_EMPTY", None) {
        let rules = vec![PolicyRule {
            pattern: vec![single("ls")],
            decision: PolicyRuleDecision::Block,
            justification: None,
            match_examples: vec![],
            not_match_examples: vec![],
            when: Some(WhenClause {
                env: "AEGIS_TEST_ABSENT_EMPTY".to_string(),
                value: String::new(),
                then: PolicyRuleDecision::Allow,
            }),
        }];
        let result = evaluate_policy_rules(&rules, "ls");
        assert!(result.matched);
        assert_eq!(result.decision, Some(PolicyRuleDecision::Block));
    }
}

#[test]
fn project_rules_prompt_with_when_then_allow_does_not_autoapprove_danger_under_protect() {
    if let Some(_completion) = in_child("AEGIS_TEST_C3_RESIDUAL_WHEN", Some("match")) {
        let workspace = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let global_dir = home.path().join(".config/aegis");
        fs::create_dir_all(&global_dir).unwrap();

        fs::write(global_dir.join("config.toml"), "mode = \"Protect\"\n").unwrap();
        fs::write(
            workspace.path().join(".aegis.toml"),
            "[[rules]]\n\
         pattern = [\"terraform\"]\n\
         decision = \"prompt\"\n\
         when = { env = \"AEGIS_TEST_C3_RESIDUAL_WHEN\", value = \"match\", then = \"allow\" }\n",
        )
        .unwrap();

        let config =
            AegisConfig::load_for(workspace.path(), Some(home.path())).expect("config must load");

        let rules_result =
            evaluate_policy_rules(&config.rules, "terraform destroy -target=module.prod.api");

        assert!(
            !rules_result.matched,
            "project-layer [[rules]] prompt+when.then=allow must be dropped so it does not match; \
         got rules_result = {:?} (merged rules = {:?})",
            rules_result, config.rules,
        );

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
            "a dropped project prompt+when.then=allow must leave a Danger command prompting \
         under Protect; got decision = {:?}",
            decision,
        );
        assert_ne!(
            decision.decision,
            PolicyAction::AutoApprove,
            "project [[rules]] prompt+when.then=allow must NOT auto-approve a Danger command; \
         got decision = {:?}",
            decision,
        );
    }
}
