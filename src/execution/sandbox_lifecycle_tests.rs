use aegis_types::SandboxStatus;

use super::*;

struct Fake {
    prepare: Result<SandboxStatus, aegis_sandbox::SandboxError>,
    spawn: Result<SandboxStatus, aegis_sandbox::SandboxError>,
    audit_error: Option<&'static str>,
    exit: i32,
    unavailable: &'static str,
    events: Vec<String>,
}

impl Fake {
    fn new(prepare: Result<SandboxStatus, aegis_sandbox::SandboxError>) -> Self {
        Self {
            prepare,
            spawn: Ok(SandboxStatus::NotConfigured),
            audit_error: None,
            exit: 0,
            unavailable: aegis::runtime::SANDBOX_REQUIRED_UNAVAILABLE_MESSAGE,
            events: Vec::new(),
        }
    }
}

impl ShellLifecycle for Fake {
    type Prepared = ();
    type Running = ();
    type AuditError = &'static str;

    fn prepare(&mut self) -> Result<((), SandboxStatus), aegis_sandbox::SandboxError> {
        std::mem::replace(
            &mut self.prepare,
            Err(aegis_sandbox::SandboxError::Required),
        )
        .map(|status| ((), status))
    }

    fn append_audit(
        &mut self,
        decision: Decision,
        status: SandboxStatus,
    ) -> Result<(), &'static str> {
        if let Some(err) = self.audit_error {
            return Err(err);
        }
        self.events.push(format!("audit:{decision:?}:{status:?}"));
        Ok(())
    }

    fn execute(&mut self, (): ()) -> Result<((), SandboxStatus), aegis_sandbox::SandboxError> {
        self.events.push("spawn".to_string());
        std::mem::replace(&mut self.spawn, Err(aegis_sandbox::SandboxError::Required))
            .map(|status| ((), status))
    }

    fn wait(&mut self, (): ()) -> i32 {
        self.events.push("wait".to_string());
        self.exit
    }

    fn report_error(&mut self, message: &str) {
        self.events.push(format!("error:{message}"));
    }

    fn unavailable_message(&self) -> &'static str {
        self.unavailable
    }
}

#[test]
fn required_unavailability_is_audited_as_blocked_and_never_executes() {
    let mut fake = Fake::new(Err(aegis_sandbox::SandboxError::Required));

    let exit_code = complete_shell_execution(Decision::Approved, &mut fake);

    assert_eq!(exit_code, EXIT_BLOCKED);
    assert_eq!(
        fake.events,
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
    let mut fake = Fake::new(Err(aegis_sandbox::SandboxError::Required));
    fake.unavailable = aegis::runtime::sandbox_unavailable_code_and_message(false, true).1;

    let exit = complete_shell_execution(Decision::Approved, &mut fake);

    assert_eq!(exit, EXIT_BLOCKED);
    assert_eq!(
        fake.events,
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
    let mut fake = Fake::new(Err(
        aegis_sandbox::SandboxError::RequiredNestedUnderOuterSandbox,
    ));

    let exit_code = complete_shell_execution(Decision::Approved, &mut fake);

    assert_eq!(exit_code, EXIT_BLOCKED);
    assert_eq!(
        fake.events,
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
    let mut fake = Fake::new(Err(aegis_sandbox::SandboxError::SetupFailed(
        "invalid profile".to_string(),
    )));

    let exit_code = complete_shell_execution(Decision::Approved, &mut fake);

    assert_eq!(exit_code, EXIT_INTERNAL);
    assert_eq!(
        fake.events,
        vec![
            "audit:Blocked:NotAttempted".to_string(),
            "error:Sandbox setup failed; command not executed: sandbox setup failed: invalid profile"
                .to_string(),
        ]
    );
}

#[test]
fn audit_failure_prevents_child_spawn() {
    let mut fake = Fake::new(Ok(SandboxStatus::NotConfigured));
    fake.audit_error = Some("permission denied");

    let exit_code = complete_shell_execution(Decision::AutoApproved, &mut fake);

    assert_eq!(exit_code, EXIT_INTERNAL);
    assert_eq!(
        fake.events,
        vec!["error:failed to write audit log: permission denied".to_string()]
    );
}

#[cfg(target_os = "linux")]
#[test]
fn missing_inner_wrapper_report_is_audited_as_blocked_before_reaping_the_child() {
    let mut fake = Fake::new(Ok(SandboxStatus::Active));
    fake.spawn = Ok(SandboxStatus::NotAttempted);

    let exit_code = complete_shell_execution(Decision::AutoApproved, &mut fake);

    assert_eq!(exit_code, EXIT_INTERNAL);
    assert_eq!(
        fake.events,
        vec![
            "spawn".to_string(),
            "audit:Blocked:NotAttempted".to_string(),
            "wait".to_string(),
            "error:Sandbox setup failed; command not executed: inner sandbox wrapper did not report status"
                .to_string(),
        ]
    );
}

#[test]
fn unconfigured_sandbox_is_silent() {
    let mut fake = Fake::new(Ok(SandboxStatus::NotConfigured));

    let exit_code = complete_shell_execution(Decision::AutoApproved, &mut fake);

    assert_eq!(exit_code, 0);
    assert_eq!(
        fake.events,
        vec![
            "audit:AutoApproved:NotConfigured".to_string(),
            "spawn".to_string(),
            "wait".to_string()
        ]
    );
}

#[cfg(target_os = "linux")]
#[test]
fn execution_audits_the_inner_wrapper_status_before_waiting_for_the_child() {
    let mut fake = Fake::new(Ok(SandboxStatus::Active));
    fake.spawn = Ok(SandboxStatus::Active);
    fake.exit = 17;

    let exit_code = complete_shell_execution(Decision::AutoApproved, &mut fake);

    assert_eq!(exit_code, 17);
    assert_eq!(
        fake.events,
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

        let mut fake = Fake::new(Err(aegis_sandbox::SandboxError::Required));
        let exit_code = complete_shell_execution(Decision::Approved, &mut fake);
        assert_eq!(exit_code, EXIT_BLOCKED, "failed for {body:?}");
        assert_eq!(
            fake.events,
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
