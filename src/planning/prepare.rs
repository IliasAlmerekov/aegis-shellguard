//! Planner preparation: resolve context, run scanner, build plan.

use tokio::runtime::Handle;

use crate::error::AegisError;
use crate::planning::types::SetupFailurePlan;
use crate::runtime::RuntimeContext;
use aegis_policy::ExecutionTransport;

/// Prepared planning dependency state shared across multiple planning requests.
pub enum PreparedPlanner {
    /// Runtime preparation succeeded and planning can proceed normally.
    Ready(Box<RuntimeContext>),
    /// Runtime preparation failed and every request must fail closed the same way.
    SetupFailure(SetupFailurePlan),
}

/// Prepare planner dependencies once and return a typed ready/fail-closed wrapper.
pub fn prepare_planner(verbose: bool, handle: Handle) -> PreparedPlanner {
    match RuntimeContext::load(verbose, handle) {
        Ok(context) => PreparedPlanner::Ready(Box::new(context)),
        Err(err) => PreparedPlanner::SetupFailure(setup_failure_from_runtime_error(
            &err,
            "",
            ExecutionTransport::Shell,
        )),
    }
}

/// Map a runtime setup failure into a typed fail-closed setup plan.
pub fn setup_failure_from_runtime_error(
    err: &AegisError,
    command: &str,
    transport: ExecutionTransport,
) -> SetupFailurePlan {
    let _ = (command, transport);

    let is_config_fault = err.is_config_fault();

    let user_message = if is_config_fault {
        format!("error: failed to load config: {err}")
    } else if let AegisError::Audit(aegis_audit::error::AuditError::Parse { path, line, .. }) = err
    {
        match line {
            Some(number) => {
                format!("error: audit log '{path}' is corrupted at line {number}: {err}")
            }
            None => format!("error: audit log '{path}' is corrupted: {err}"),
        }
    } else {
        format!("error: failed to initialize runtime: {err}")
    };

    SetupFailurePlan::new(user_message, is_config_fault)
}

#[cfg(test)]
mod tests {
    use crate::error::AegisError;
    use aegis_config::error::ConfigError;

    fn bad_config_error() -> AegisError {
        AegisError::Config(ConfigError::Config("bad config".to_string()))
    }

    #[test]
    fn config_errors_become_setup_failure_plans() {
        let plan = super::setup_failure_from_runtime_error(
            &bad_config_error(),
            "echo hi",
            aegis_policy::ExecutionTransport::Shell,
        );

        assert!(plan.user_message().contains("failed to load config"));
        assert!(plan.is_config_fault());
    }

    #[test]
    fn corrupted_audit_log_becomes_a_distinct_user_message() {
        let source = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err = AegisError::Audit(aegis_audit::error::AuditError::Parse {
            path: "/home/user/.aegis/audit.jsonl".to_string(),
            line: Some(7),
            source,
        });
        let plan = super::setup_failure_from_runtime_error(
            &err,
            "echo hi",
            aegis_policy::ExecutionTransport::Shell,
        );

        assert!(plan.user_message().contains("audit log"));
        assert!(plan.user_message().contains("corrupted"));
        assert!(!plan.is_config_fault());
    }

    #[test]
    fn prepared_setup_failure_replays_same_planning_outcome_for_every_request() {
        let prepared =
            super::PreparedPlanner::SetupFailure(super::setup_failure_from_runtime_error(
                &bad_config_error(),
                "echo hi",
                aegis_policy::ExecutionTransport::Shell,
            ));

        let first = prepared.plan(crate::planning::PlanningRequest {
            command: "echo one",
            cwd_state: crate::planning::CwdState::Resolved(std::path::PathBuf::from(".")),
            transport: aegis_policy::ExecutionTransport::Shell,
            ci_detected: false,
        });
        let second = prepared.plan(crate::planning::PlanningRequest {
            command: "echo two",
            cwd_state: crate::planning::CwdState::Resolved(std::path::PathBuf::from(".")),
            transport: aegis_policy::ExecutionTransport::Shell,
            ci_detected: false,
        });

        assert!(matches!(
            first,
            crate::planning::PlanningOutcome::SetupFailure(_)
        ));
        assert!(matches!(
            second,
            crate::planning::PlanningOutcome::SetupFailure(_)
        ));
    }
}
