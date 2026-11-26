use aegis_types::Assessment;
use aegis_types::{AllowlistOverrideLevel, CiPolicy, Mode, PolicyRuleDecision, SnapshotPolicy};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionTransport {
    Shell,
    Watch,
    Evaluation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyCiState {
    pub detected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyAllowlistResult {
    pub matched: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyBlocklistResult {
    pub matched: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PolicyRulesResult {
    pub matched: bool,
    pub decision: Option<PolicyRuleDecision>,
    pub justification: Option<std::borrow::Cow<'static, str>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyConfigFlags {
    pub ci_policy: CiPolicy,
    pub allowlist_override_level: AllowlistOverrideLevel,
    pub snapshot_policy: SnapshotPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyExecutionContext<'a> {
    pub transport: ExecutionTransport,
    pub applicable_snapshot_plugins: &'a [&'static str],
}

#[derive(Clone)]
pub struct PolicyInput<'a> {
    pub assessment: &'a Assessment,
    pub mode: Mode,
    pub ci_state: PolicyCiState,
    pub allowlist: PolicyAllowlistResult,
    pub blocklist: PolicyBlocklistResult,
    pub config_flags: PolicyConfigFlags,
    pub execution_context: PolicyExecutionContext<'a>,
    pub rules: PolicyRulesResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyAction {
    AutoApprove,
    Prompt,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockReason {
    IntrinsicRiskBlock,
    StrictPolicy,
    ProtectCiPolicy,
    BlocklistOverride,
    PolicyRulesOverride,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyRationale {
    AuditMode,
    SafeCommand,
    AllowlistOverride,
    RequiresConfirmation,
    AnalysisConfirmationRequired,
    AnalysisOverrideRequired,
    IntrinsicRiskBlock,
    ProtectCiPolicy,
    StrictPolicy,
    BlocklistOverride,
    PolicyRulesOverride,
}

impl PolicyRationale {
    #[must_use]
    pub fn block_reason(self) -> Option<BlockReason> {
        match self {
            Self::IntrinsicRiskBlock => Some(BlockReason::IntrinsicRiskBlock),
            Self::ProtectCiPolicy => Some(BlockReason::ProtectCiPolicy),
            Self::StrictPolicy => Some(BlockReason::StrictPolicy),
            Self::BlocklistOverride => Some(BlockReason::BlocklistOverride),
            Self::PolicyRulesOverride => Some(BlockReason::PolicyRulesOverride),
            Self::AuditMode
            | Self::SafeCommand
            | Self::AllowlistOverride
            | Self::RequiresConfirmation
            | Self::AnalysisConfirmationRequired
            | Self::AnalysisOverrideRequired => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyDecision {
    pub decision: PolicyAction,
    pub rationale: PolicyRationale,
    pub requires_confirmation: bool,
    pub snapshots_required: bool,
    pub confinement_required: bool,
    pub allowlist_effective: bool,
}

impl PolicyDecision {
    #[must_use]
    pub fn block_reason(self) -> Option<BlockReason> {
        self.rationale.block_reason()
    }
}
