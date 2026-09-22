use std::path::PathBuf;

use aegis::planning::{
    CwdState, ExecutionDisposition, PlanningOutcome, PlanningRequest, SnapshotPlan,
};
use aegis::runtime::RuntimeContext;
use aegis_config::AegisConfig;
use aegis_policy::ExecutionTransport;
use aegis_types::{Mode, SnapshotPolicy};
use tokio::runtime::Handle;

fn test_handle() -> Handle {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let handle = runtime.handle().clone();
    std::mem::forget(runtime);
    handle
}

#[test]
fn effect_opaque_non_danger_plan_requests_recovery_snapshot() {
    let mut config = AegisConfig::default();
    config.mode = Mode::Protect;
    config.snapshot_policy = SnapshotPolicy::Selective;
    config.auto_snapshot_git = false;
    config.auto_snapshot_docker = false;
    let context = RuntimeContext::new(config, test_handle()).unwrap();

    let outcome = aegis::planning::plan_with_context(
        &context,
        PlanningRequest {
            command: "sh ./cleanup.sh",
            cwd_state: CwdState::Resolved(PathBuf::from(".")),
            transport: ExecutionTransport::Shell,
            ci_detected: false,
        },
    );

    let PlanningOutcome::Planned(plan) = outcome else {
        panic!("effect-opaque command must produce a plan");
    };
    assert_eq!(
        plan.execution_disposition(),
        ExecutionDisposition::RequiresApproval
    );
    assert_eq!(
        plan.snapshot_plan(),
        SnapshotPlan::Required {
            applicable_plugins: Vec::new()
        }
    );
}
