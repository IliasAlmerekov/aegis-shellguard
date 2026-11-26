use std::cell::RefCell;

use aegis_types::SandboxStatus;

use super::*;

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
        |_: ()| panic!("required preparation must not execute"),
        |_: ()| panic!("required preparation must not wait"),
        |message| events.borrow_mut().push(format!("error:{message}")),
        || aegis::runtime::SANDBOX_REQUIRED_UNAVAILABLE_MESSAGE,
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
fn apparmor_unavailability_blocks_and_audits() {
    let events = RefCell::new(Vec::new());
    let exit = complete_shell_execution(
        Decision::Approved,
        || Err::<((), SandboxStatus), _>(aegis_sandbox::SandboxError::Required),
        |decision, status| {
            events
                .borrow_mut()
                .push(format!("audit:{decision:?}:{status:?}"));
            Ok::<_, String>(())
        },
        |_: ()| panic!(),
        |_: ()| panic!(),
        |message| events.borrow_mut().push(format!("error:{message}")),
        || aegis::runtime::sandbox_unavailable_code_and_message(false, true).1,
    );
    assert_eq!(exit, EXIT_BLOCKED);
    assert_eq!(
        events.into_inner(),
        vec![
            "audit:Blocked:Unavailable".to_string(),
            format!(
                "error:{}",
                aegis::runtime::SANDBOX_APPARMOR_UNAVAILABLE_MESSAGE
            )
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
        |_: ()| panic!("required preparation must not execute"),
        |_: ()| panic!("required preparation must not wait"),
        |message| events.borrow_mut().push(format!("error:{message}")),
        || aegis::runtime::SANDBOX_REQUIRED_UNAVAILABLE_MESSAGE,
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
        |_: ()| panic!("failed preparation must not execute"),
        |_: ()| panic!("failed preparation must not wait"),
        |message| events.borrow_mut().push(format!("error:{message}")),
        || aegis::runtime::SANDBOX_REQUIRED_UNAVAILABLE_MESSAGE,
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
fn audit_failure_prevents_child_spawn() {
    let events = RefCell::new(Vec::new());

    let exit_code = complete_shell_execution(
        Decision::AutoApproved,
        || Ok(((), SandboxStatus::NotConfigured)),
        |_decision, _status| Err::<(), _>("permission denied".to_string()),
        |()| {
            events.borrow_mut().push("spawn".to_string());
            Ok(((), SandboxStatus::NotConfigured))
        },
        |()| {
            events.borrow_mut().push("wait".to_string());
            0
        },
        |message| events.borrow_mut().push(format!("error:{message}")),
        || aegis::runtime::SANDBOX_REQUIRED_UNAVAILABLE_MESSAGE,
    );

    assert_eq!(exit_code, EXIT_INTERNAL);
    assert_eq!(
        events.into_inner(),
        vec!["error:failed to write audit log: permission denied".to_string()]
    );
}

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
        |()| {
            events.borrow_mut().push("spawn".to_string());
            Ok(((), SandboxStatus::NotAttempted))
        },
        |()| {
            events.borrow_mut().push("reap".to_string());
            0
        },
        |message| events.borrow_mut().push(format!("error:{message}")),
        || aegis::runtime::SANDBOX_REQUIRED_UNAVAILABLE_MESSAGE,
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
        |()| {
            events.borrow_mut().push("spawn".to_string());
            Ok(((), SandboxStatus::NotConfigured))
        },
        |()| {
            events.borrow_mut().push("wait".to_string());
            0
        },
        |message| events.borrow_mut().push(format!("error:{message}")),
        || aegis::runtime::SANDBOX_REQUIRED_UNAVAILABLE_MESSAGE,
    );

    assert_eq!(exit_code, 0);
    assert_eq!(
        events.into_inner(),
        vec!["spawn".to_string(), "wait".to_string()]
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
        |()| {
            events.borrow_mut().push("spawn".to_string());
            Ok(((), SandboxStatus::Active))
        },
        |()| {
            events.borrow_mut().push("wait".to_string());
            17
        },
        |message| events.borrow_mut().push(format!("error:{message}")),
        || aegis::runtime::SANDBOX_REQUIRED_UNAVAILABLE_MESSAGE,
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

#[test]
fn unavailable_sandbox_blocks_for_default_and_empty_sandbox_section() {
    let dir = tempfile::TempDir::new().unwrap();
    for body in ["", "[sandbox]\n"] {
        let config: aegis_config::AegisConfig = toml::from_str(body).unwrap();
        assert_ne!(config.mode, aegis_types::Mode::Audit);
        let sandbox = aegis::runtime::sandbox_ceiling(&config, dir.path())
            .expect("every config outside Mode::Audit must build SandboxConfig");
        assert!(
            !sandbox.config.trusted_roots.is_empty(),
            "SandboxConfig must be populated for {body:?}"
        );

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
            |_: ()| panic!("unavailable sandbox must not execute"),
            |_: ()| panic!("unavailable sandbox must not wait"),
            |message| events.borrow_mut().push(format!("error:{message}")),
            || aegis::runtime::SANDBOX_REQUIRED_UNAVAILABLE_MESSAGE,
        );
        assert_eq!(exit_code, EXIT_BLOCKED, "failed for {body:?}");
        assert_eq!(
            events.into_inner(),
            vec![
                "audit:Blocked:Unavailable".to_string(),
                format!(
                    "error:{}",
                    aegis::runtime::SANDBOX_REQUIRED_UNAVAILABLE_MESSAGE
                ),
            ],
            "failed for {body:?}"
        );
    }
}
