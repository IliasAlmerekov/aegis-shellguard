#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable
    )
)]

mod engine;
mod types;

pub use engine::{DefaultPolicyEngine, PolicyEngine, evaluate_policy};
pub use types::{
    BlockReason, ExecutionTransport, PolicyAction, PolicyAllowlistResult, PolicyBlocklistResult,
    PolicyCiState, PolicyConfigFlags, PolicyDecision, PolicyExecutionContext, PolicyInput,
    PolicyRationale, PolicyRulesResult,
};
