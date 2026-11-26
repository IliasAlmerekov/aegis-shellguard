#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable
    )
)]

pub use aegis_policy::{BlockReason, ExecutionTransport, PolicyAction, PolicyRationale};

use aegis_config::ConfigSourceLayer;
use aegis_types::Mode;
use aegis_types::{AssessmentBasis, DecisionSource, RiskLevel};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandExplanation {
    pub scan: ScanExplanation,
    pub policy: PolicyExplanation,
    pub context: ExecutionContextExplanation,
    pub outcome: Option<ExecutionOutcomeExplanation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanExplanation {
    pub highest_risk: RiskLevel,
    pub decision_source: DecisionSource,
    #[serde(skip)]
    pub basis: AssessmentBasis,
    pub matched_patterns: Vec<ExplainedPatternMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExplainedPatternMatch {
    pub id: String,
    pub risk: RiskLevel,
    pub description: String,
    pub matched_text: String,
    pub justification: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyExplanation {
    pub action: PolicyAction,
    pub rationale: PolicyRationale,
    pub requires_confirmation: bool,
    pub snapshots_required: bool,
    pub allowlist_effective: bool,
    pub block_reason: Option<BlockReason>,
}

impl PolicyExplanation {
    #[must_use]
    pub fn concise_reason_label(&self) -> &'static str {
        match self.rationale {
            PolicyRationale::AuditMode => "audit mode auto-approved this command",
            PolicyRationale::SafeCommand => "safe command",
            PolicyRationale::AllowlistOverride => "allowlist override applied",
            PolicyRationale::RequiresConfirmation => "requires confirmation",
            PolicyRationale::AnalysisConfirmationRequired => {
                "analysis requires one-time confirmation"
            }
            PolicyRationale::AnalysisOverrideRequired => "analysis override required",
            PolicyRationale::IntrinsicRiskBlock => "blocked by intrinsic risk",
            PolicyRationale::ProtectCiPolicy => "blocked by protect-mode CI policy",
            PolicyRationale::StrictPolicy => "blocked by strict mode",
            PolicyRationale::BlocklistOverride => "blocked by user-defined blocklist rule",
            PolicyRationale::PolicyRulesOverride => "overridden by typed policy rule",
        }
    }
}

pub const NO_ROLLBACK_DISCLOSURE: &str =
    "No Snapshot provider applies: this command cannot be rolled back.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionContextExplanation {
    pub mode: Mode,
    pub transport: ExecutionTransport,
    pub ci_detected: bool,
    pub allowlist_match: Option<AllowlistExplanation>,
    pub applicable_snapshot_plugins: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub git_targets: Vec<GitTargetExplanation>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub recovery_opt_out: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_rollback_disclosure: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum GitTargetExplanation {
    Resolved { path: String },
    Unresolved { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllowlistExplanation {
    pub pattern: String,
    pub reason: String,
    pub source_layer: ConfigSourceLayer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionOutcomeExplanation {
    pub decision: ExecutionDecisionExplanation,
    pub snapshots: Vec<SnapshotOutcomeExplanation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionDecisionExplanation {
    Approved,
    Denied,
    AutoApproved,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotOutcomeExplanation {
    pub plugin: String,
    pub snapshot_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_concise_reason_labels_omit_language_wording() {
        let confirm = PolicyExplanation {
            action: PolicyAction::Prompt,
            rationale: PolicyRationale::AnalysisConfirmationRequired,
            requires_confirmation: true,
            snapshots_required: false,
            allowlist_effective: false,
            block_reason: None,
        };
        assert_eq!(
            confirm.concise_reason_label(),
            "analysis requires one-time confirmation"
        );
        assert!(
            !confirm
                .concise_reason_label()
                .to_ascii_lowercase()
                .contains("language")
        );

        let override_exp = PolicyExplanation {
            rationale: PolicyRationale::AnalysisOverrideRequired,
            ..confirm
        };
        assert_eq!(
            override_exp.concise_reason_label(),
            "analysis override required"
        );
        assert!(
            !override_exp
                .concise_reason_label()
                .to_ascii_lowercase()
                .contains("language")
        );
    }
}
