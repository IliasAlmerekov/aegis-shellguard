//! Stable active-channel diagnostics for optional Sandbox execution, and the
//! Sandbox config built for one working directory.

use std::path::Path;

/// Stable diagnostic code for optional Sandbox degradation.
pub const SANDBOX_UNAVAILABLE_CODE: &str = "sandbox_unavailable";

/// Stable message emitted when optional confinement is unavailable.
///
/// Names no config remedy: `sandbox.required` left the config contract in
/// #240 and is now accepted-but-ignored, so telling a user to set it would be
/// stale, misleading advice.
pub const SANDBOX_UNAVAILABLE_MESSAGE: &str =
    "Sandbox unavailable; proceeding without confinement.";

/// Stable diagnostic code for Sandbox unavailability caused by WSL1, which
/// cannot create the user namespaces bubblewrap needs.
pub const SANDBOX_WSL1_UNAVAILABLE_CODE: &str = "sandbox_wsl1_unavailable";

/// Stable message emitted when the Linux sandbox is unavailable because the
/// host is WSL1. Names the practical remedy (use WSL2) rather than the generic
/// unavailability message.
pub const SANDBOX_WSL1_UNAVAILABLE_MESSAGE: &str = "Sandbox unavailable: WSL1 cannot create the user namespaces bubblewrap needs. Use WSL2 for sandboxed commands.";

/// Stable diagnostic code for required Sandbox unavailability.
pub const SANDBOX_REQUIRED_UNAVAILABLE_CODE: &str = "sandbox_required_unavailable";

/// Stable message emitted when required confinement blocks execution for a
/// generic reason (missing or broken `sandbox-exec`/`bwrap`, or any other
/// cause that names no practical remedy).
pub const SANDBOX_REQUIRED_UNAVAILABLE_MESSAGE: &str =
    "Required Sandbox unavailable; command not executed.";

/// Stable diagnostic code for required Sandbox unavailability caused
/// specifically by nesting under an already-active outer sandbox.
pub const SANDBOX_REQUIRED_NESTED_UNAVAILABLE_CODE: &str = "sandbox_required_nested_unavailable";

/// Stable message emitted when required confinement blocks execution because
/// this process is already nested under an active outer sandbox (macOS
/// Seatbelt refuses a nested `sandbox_apply`; see ADR-029 §8/amendment) —
/// distinct from [`SANDBOX_REQUIRED_UNAVAILABLE_MESSAGE`] because this cause
/// has a practical remedy to name.
///
/// Names both known outer-agent cases (#262's measured asymmetry) rather than
/// presenting the Claude Code remedy as universal: Claude Code ships with its
/// own sandbox off by default, so a user who hits this can disable `/sandbox`
/// and retry; Codex requires its own sandbox to launch at all on macOS, so
/// every Codex-on-macOS session hits this unconditionally, with no bypass to
/// name.
pub const SANDBOX_REQUIRED_NESTED_UNAVAILABLE_MESSAGE: &str = "Required Sandbox unavailable: this process is already nested under an active outer sandbox, which refuses a second, nested confinement layer. Under Claude Code, disable /sandbox and retry. Under Codex, this is unconditional on macOS today and has no bypass. Command not executed.";

/// The effective Sandbox config for one working directory, with the warnings
/// the ceiling computation raised.
pub struct SandboxCeiling {
    /// What enforcement receives.
    pub config: aegis_sandbox::SandboxConfig,
    /// Typed ceiling warnings (config level).
    pub warnings: Vec<aegis_config::ceiling::CeilingWarning>,
}

/// Build the Sandbox config for `cwd`, or `None` when the Sandbox is disabled.
///
/// The workspace (a `.git` search) is resolved here, only when the Sandbox is
/// enabled, never on the disabled command path.
pub fn sandbox_ceiling(config: &aegis_config::AegisConfig, cwd: &Path) -> Option<SandboxCeiling> {
    use aegis_config::ceiling::{WriteRoot, compute_trusted_ceiling};

    if !config.sandbox.enabled {
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
            required: config.sandbox.required,
        },
        warnings: ceiling.warnings,
    })
}

/// Diagnostics for `aegis status` and `config validate`: the config-level
/// ceiling warnings plus the roots enforcement would omit (a symlink that
/// resolves outside the trusted roots). Never printed on the `-c` path.
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
