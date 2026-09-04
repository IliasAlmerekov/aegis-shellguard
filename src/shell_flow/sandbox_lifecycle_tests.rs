use std::cell::RefCell;

use aegis_types::SandboxStatus;

use super::*;

#[test]
fn optional_unavailability_is_audited_then_warned_before_execution() {
    let events = RefCell::new(Vec::new());

    let exit_code = complete_shell_execution(
        Decision::AutoApproved,
        || Ok(((), SandboxStatus::Unavailable)),
        |decision, status| {
            events
                .borrow_mut()
                .push(format!("audit:{decision:?}:{status:?}"));
            Ok::<_, String>(())
        },
        |message| events.borrow_mut().push(format!("warning:{message}")),
        |()| {
            events.borrow_mut().push("spawn".to_string());
            Ok(((), SandboxStatus::Unavailable))
        },
        |()| {
            events.borrow_mut().push("wait".to_string());
            0
        },
        |message| events.borrow_mut().push(format!("error:{message}")),
    );

    assert_eq!(exit_code, 0);
    let events = events.into_inner();
    assert_eq!(events.len(), 4);
    assert_eq!(events[0], "audit:AutoApproved:Unavailable");
    // The warning names the WSL1 cause on a WSL1 host and stays generic
    // elsewhere, so the lifecycle contract is membership in the sanctioned
    // pair, not a fixed message.
    assert!(
        events[1] == format!("warning:{}", aegis::runtime::SANDBOX_UNAVAILABLE_MESSAGE)
            || events[1]
                == format!(
                    "warning:{}",
                    aegis::runtime::SANDBOX_WSL1_UNAVAILABLE_MESSAGE
                ),
        "unexpected unavailable warning: {}",
        events[1]
    );
    assert_eq!(events[2], "spawn");
    assert_eq!(events[3], "wait");
}

#[test]
fn required_unavailability_is_audited_as_blocked_and_never_executes() {
    let events = RefCell::new(Vec::new());

    let exit_code = complete_shell_execution(
        Decision::Approved,
        || Err::<((), SandboxStatus), _>(aegis_sandbox::SandboxError::Required),
        |decision, status| {
            events
                .borrow_mut()
                .push(format!("audit:{decision:?}:{status:?}"));
            Ok::<_, String>(())
        },
        |message| events.borrow_mut().push(format!("warning:{message}")),
        |_: ()| unreachable!("required preparation must not execute"),
        |_: ()| unreachable!("required preparation must not wait"),
        |message| events.borrow_mut().push(format!("error:{message}")),
    );

    assert_eq!(exit_code, EXIT_BLOCKED);
    assert_eq!(
        events.into_inner(),
        vec![
            "audit:Blocked:Unavailable".to_string(),
            format!(
                "error:{}",
                aegis::runtime::SANDBOX_REQUIRED_UNAVAILABLE_MESSAGE
            ),
        ]
    );
}

#[test]
fn required_nested_unavailability_names_the_cause_and_never_executes() {
    let events = RefCell::new(Vec::new());

    let exit_code = complete_shell_execution(
        Decision::Approved,
        || {
            Err::<((), SandboxStatus), _>(
                aegis_sandbox::SandboxError::RequiredNestedUnderOuterSandbox,
            )
        },
        |decision, status| {
            events
                .borrow_mut()
                .push(format!("audit:{decision:?}:{status:?}"));
            Ok::<_, String>(())
        },
        |message| events.borrow_mut().push(format!("warning:{message}")),
        |_: ()| unreachable!("required preparation must not execute"),
        |_: ()| unreachable!("required preparation must not wait"),
        |message| events.borrow_mut().push(format!("error:{message}")),
    );

    assert_eq!(exit_code, EXIT_BLOCKED);
    assert_eq!(
        events.into_inner(),
        vec![
            "audit:Blocked:Unavailable".to_string(),
            format!(
                "error:{}",
                aegis::runtime::SANDBOX_REQUIRED_NESTED_UNAVAILABLE_MESSAGE
            ),
        ]
    );
}

#[test]
fn setup_failure_is_audited_as_not_attempted_and_never_executes() {
    let events = RefCell::new(Vec::new());

    let exit_code = complete_shell_execution(
        Decision::Approved,
        || {
            Err::<((), SandboxStatus), _>(aegis_sandbox::SandboxError::SetupFailed(
                "invalid profile".to_string(),
            ))
        },
        |decision, status| {
            events
                .borrow_mut()
                .push(format!("audit:{decision:?}:{status:?}"));
            Ok::<_, String>(())
        },
        |message| events.borrow_mut().push(format!("warning:{message}")),
        |_: ()| unreachable!("failed preparation must not execute"),
        |_: ()| unreachable!("failed preparation must not wait"),
        |message| events.borrow_mut().push(format!("error:{message}")),
    );

    assert_eq!(exit_code, EXIT_INTERNAL);
    assert_eq!(
            events.into_inner(),
            vec![
                "audit:Blocked:NotAttempted".to_string(),
                "error:Sandbox setup failed; command not executed: sandbox setup failed: invalid profile"
                    .to_string(),
            ]
        );
}

#[test]
fn audit_failure_prevents_optional_warning_and_child_spawn() {
    let events = RefCell::new(Vec::new());

    let exit_code = complete_shell_execution(
        Decision::AutoApproved,
        || Ok(((), SandboxStatus::Unavailable)),
        |_decision, _status| Err::<(), _>("permission denied".to_string()),
        |message| events.borrow_mut().push(format!("warning:{message}")),
        |()| {
            events.borrow_mut().push("spawn".to_string());
            Ok(((), SandboxStatus::Unavailable))
        },
        |()| {
            events.borrow_mut().push("wait".to_string());
            0
        },
        |message| events.borrow_mut().push(format!("error:{message}")),
    );

    assert_eq!(exit_code, EXIT_INTERNAL);
    assert_eq!(
        events.into_inner(),
        vec!["error:failed to write audit log: permission denied".to_string()]
    );
}

// Linux-only: a prepared `Active` status routes through the inner Landlock
// wrapper's report only on Linux (`needs_inner_report` in
// `complete_shell_execution`). Every other platform keeps the M1
// audit-before-spawn ordering, which the next two cases would contradict.
#[cfg(target_os = "linux")]
#[test]
fn missing_inner_wrapper_report_is_audited_as_blocked_before_reaping_the_child() {
    let events = RefCell::new(Vec::new());

    let exit_code = complete_shell_execution(
        Decision::AutoApproved,
        || Ok(((), SandboxStatus::Active)),
        |decision, status| {
            events
                .borrow_mut()
                .push(format!("audit:{decision:?}:{status:?}"));
            Ok::<_, String>(())
        },
        |message| events.borrow_mut().push(format!("warning:{message}")),
        |()| {
            events.borrow_mut().push("spawn".to_string());
            Ok(((), SandboxStatus::NotAttempted))
        },
        |()| {
            events.borrow_mut().push("reap".to_string());
            0
        },
        |message| events.borrow_mut().push(format!("error:{message}")),
    );

    assert_eq!(exit_code, EXIT_INTERNAL);
    assert_eq!(
        events.into_inner(),
        vec![
            "spawn".to_string(),
            "audit:Blocked:NotAttempted".to_string(),
            "reap".to_string(),
            "error:Sandbox setup failed; command not executed: inner sandbox wrapper did not report status"
                .to_string(),
        ]
    );
}

#[test]
fn unconfigured_sandbox_is_silent() {
    let events = RefCell::new(Vec::new());

    let exit_code = complete_shell_execution(
        Decision::AutoApproved,
        || Ok(((), SandboxStatus::NotConfigured)),
        |_decision, _status| Ok::<_, String>(()),
        |message| events.borrow_mut().push(format!("warning:{message}")),
        |()| {
            events.borrow_mut().push("spawn".to_string());
            Ok(((), SandboxStatus::NotConfigured))
        },
        |()| {
            events.borrow_mut().push("wait".to_string());
            0
        },
        |message| events.borrow_mut().push(format!("error:{message}")),
    );

    assert_eq!(exit_code, 0);
    assert_eq!(
        events.into_inner(),
        vec!["spawn".to_string(), "wait".to_string()]
    );
}

#[test]
fn unavailable_warning_names_the_wsl1_cause_only_on_wsl1_hosts() {
    assert_eq!(
        sandbox_unavailable_warning_for(true),
        aegis::runtime::SANDBOX_WSL1_UNAVAILABLE_MESSAGE
    );
    assert_eq!(
        sandbox_unavailable_warning_for(false),
        aegis::runtime::SANDBOX_UNAVAILABLE_MESSAGE
    );
}

#[cfg(target_os = "linux")]
#[test]
fn execution_audits_the_inner_wrapper_status_before_waiting_for_the_child() {
    let events = RefCell::new(Vec::new());

    let exit_code = complete_shell_execution(
        Decision::AutoApproved,
        || Ok(((), SandboxStatus::Active)),
        |decision, status| {
            events
                .borrow_mut()
                .push(format!("audit:{decision:?}:{status:?}"));
            Ok::<_, String>(())
        },
        |message| events.borrow_mut().push(format!("warning:{message}")),
        |()| {
            events.borrow_mut().push("spawn".to_string());
            Ok(((), SandboxStatus::Active))
        },
        |()| {
            events.borrow_mut().push("wait".to_string());
            17
        },
        |message| events.borrow_mut().push(format!("error:{message}")),
    );

    assert_eq!(exit_code, 17);
    assert_eq!(
        events.into_inner(),
        vec![
            "spawn".to_string(),
            "audit:AutoApproved:Active".to_string(),
            "wait".to_string(),
        ]
    );
}
