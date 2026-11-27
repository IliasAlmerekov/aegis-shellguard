use std::borrow::Cow;
use std::path::Path;

use aegis_types::{Decision, EXIT_BLOCKED, EXIT_INTERNAL, SandboxStatus};

pub const SANDBOX_WSL1_UNAVAILABLE_CODE: &str = "sandbox_wsl1_unavailable";

pub const SANDBOX_WSL1_UNAVAILABLE_MESSAGE: &str = "Sandbox unavailable: WSL1 cannot create the user namespaces bubblewrap needs. Use WSL2 for sandboxed commands.";

pub const SANDBOX_APPARMOR_UNAVAILABLE_CODE: &str = "sandbox_apparmor_unavailable";

pub const SANDBOX_APPARMOR_UNAVAILABLE_MESSAGE: &str = "Sandbox unavailable: kernel.apparmor_restrict_unprivileged_userns=1 may prevent bubblewrap from creating user namespaces. Use a system bwrap with an AppArmor profile permitting user namespaces, or set the sysctl to 0 (reduces system security). Command not executed.";

pub const SANDBOX_REQUIRED_UNAVAILABLE_CODE: &str = "sandbox_required_unavailable";

pub const SANDBOX_REQUIRED_UNAVAILABLE_MESSAGE: &str =
    "Required Sandbox unavailable; command not executed.";

pub fn sandbox_unavailable_code_and_message(
    is_wsl1: bool,
    apparmor_restricted: bool,
) -> (&'static str, &'static str) {
    if is_wsl1 {
        (
            SANDBOX_WSL1_UNAVAILABLE_CODE,
            SANDBOX_WSL1_UNAVAILABLE_MESSAGE,
        )
    } else if apparmor_restricted {
        (
            SANDBOX_APPARMOR_UNAVAILABLE_CODE,
            SANDBOX_APPARMOR_UNAVAILABLE_MESSAGE,
        )
    } else {
        (
            SANDBOX_REQUIRED_UNAVAILABLE_CODE,
            SANDBOX_REQUIRED_UNAVAILABLE_MESSAGE,
        )
    }
}

pub const SANDBOX_REQUIRED_NESTED_UNAVAILABLE_CODE: &str = "sandbox_required_nested_unavailable";

pub const SANDBOX_REQUIRED_NESTED_UNAVAILABLE_MESSAGE: &str = "Required Sandbox unavailable: this process is already nested under an active outer sandbox, which refuses a second, nested confinement layer. Under Claude Code, disable /sandbox and retry. Under Codex, this is unconditional on macOS today and has no bypass. Command not executed.";

pub struct SandboxCeiling {
    pub config: aegis_sandbox::SandboxConfig,
    pub warnings: Vec<aegis_config::ceiling::CeilingWarning>,
}

pub fn sandbox_ceiling(config: &aegis_config::AegisConfig, cwd: &Path) -> Option<SandboxCeiling> {
    use aegis_config::ceiling::{WriteRoot, compute_trusted_ceiling};

    if config.mode == aegis_types::Mode::Audit {
        return None;
    }
    let ceiling = compute_trusted_ceiling(&config.sandbox, cwd);
    let to_sandbox = |roots: &[WriteRoot]| {
        roots
            .iter()
            .map(|root| aegis_sandbox::WriteRoot {
                path: root.path.clone(),
                layer: root.layer,
                location: root.location.clone(),
            })
            .collect::<Vec<_>>()
    };
    Some(SandboxCeiling {
        config: aegis_sandbox::SandboxConfig {
            allow_write: to_sandbox(&ceiling.roots),
            trusted_roots: to_sandbox(&ceiling.trusted),
            allow_network: config.sandbox.allow_network,
        },
        warnings: ceiling.warnings,
    })
}

pub fn ceiling_issues(ceiling: &SandboxCeiling) -> Vec<aegis_config::validate::ValidationIssue> {
    use aegis_config::ceiling::{CeilingOmitReason, CeilingWarning, PathOmitted};
    use aegis_sandbox::OmitReason;

    let enforcement = aegis_sandbox::resolve_allow_write(&ceiling.config)
        .omitted
        .into_iter()
        .map(|omitted| {
            CeilingWarning::PathOmitted(PathOmitted {
                reason: match omitted.reason {
                    OmitReason::NotFound => CeilingOmitReason::NotFound,
                    OmitReason::OutsideTrustedCeiling => CeilingOmitReason::OutsideTrustedCeiling,
                },
                layer: omitted.root.layer,
                location: omitted.root.location,
                path: omitted.root.path,
            })
        });
    ceiling
        .warnings
        .iter()
        .cloned()
        .chain(enforcement)
        .map(|warning| aegis_config::validate::ceiling_issue(&warning))
        .collect()
}

pub fn sandbox_status_before_preparation(
    sandbox_config: Option<&aegis_sandbox::SandboxConfig>,
) -> aegis_types::SandboxStatus {
    if sandbox_config.is_some() {
        aegis_types::SandboxStatus::NotAttempted
    } else {
        aegis_types::SandboxStatus::NotConfigured
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxFailure {
    RequiredBlocked,
    RequiredBlockedNested,
    SetupFailed(String),
}

impl SandboxFailure {
    pub fn record<E>(
        error: &aegis_sandbox::SandboxError,
        append_audit: impl FnOnce(Decision, SandboxStatus) -> Result<(), E>,
    ) -> Result<Self, E> {
        let failure = match error {
            aegis_sandbox::SandboxError::Required => Self::RequiredBlocked,
            aegis_sandbox::SandboxError::RequiredNestedUnderOuterSandbox => {
                Self::RequiredBlockedNested
            }
            other => Self::SetupFailed(other.to_string()),
        };
        append_audit(Decision::Blocked, failure.audit_status())?;
        Ok(failure)
    }

    fn audit_status(&self) -> SandboxStatus {
        match self {
            Self::RequiredBlocked | Self::RequiredBlockedNested => SandboxStatus::Unavailable,
            Self::SetupFailed(_) => SandboxStatus::NotAttempted,
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Self::RequiredBlocked | Self::RequiredBlockedNested => EXIT_BLOCKED,
            Self::SetupFailed(_) => EXIT_INTERNAL,
        }
    }

    pub fn message(&self, unavailable: impl FnOnce() -> &'static str) -> Cow<'static, str> {
        match self {
            Self::RequiredBlocked => Cow::Borrowed(unavailable()),
            Self::RequiredBlockedNested => {
                Cow::Borrowed(SANDBOX_REQUIRED_NESTED_UNAVAILABLE_MESSAGE)
            }
            Self::SetupFailed(err) => Cow::Owned(setup_failed_message(err)),
        }
    }
}

pub fn setup_failed_message(err: &impl std::fmt::Display) -> String {
    format!("Sandbox setup failed; command not executed: {err}")
}

#[cfg(test)]
mod status_tests {
    use super::sandbox_status_before_preparation;
    use aegis_types::SandboxStatus;

    #[test]
    fn status_before_preparation_reflects_configuration() {
        assert_eq!(
            sandbox_status_before_preparation(None),
            SandboxStatus::NotConfigured
        );
        assert_eq!(
            sandbox_status_before_preparation(Some(&aegis_sandbox::SandboxConfig::default())),
            SandboxStatus::NotAttempted
        );
    }
}
