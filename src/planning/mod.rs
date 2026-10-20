//! Orchestration layer that wraps the pure policy engine.

pub mod core;
pub mod git_target;
#[cfg(test)]
mod git_target_plan_cd_tests;
#[cfg(test)]
mod git_target_plan_tests;
pub mod policy_rules;
pub mod prepare;
pub mod types;

pub use git_target::{GitTarget, GitTargetPlan, GitTargetUnresolved};
pub use policy_rules::evaluate_policy_rules;

pub use core::{PlanningRequest, plan_with_context, plan_with_context_async};
pub use prepare::{PreparedPlanner, prepare_planner, setup_failure_from_runtime_error};
pub use types::{
    CwdState, DecisionContext, ExecutionDisposition, InterceptionPlan, PlanningOutcome,
    SetupFailurePlan, SnapshotPlan,
};
