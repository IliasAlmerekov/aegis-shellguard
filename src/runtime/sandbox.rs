//! Stable active-channel diagnostics for optional Sandbox execution.

/// Stable diagnostic code for optional Sandbox degradation.
pub const SANDBOX_UNAVAILABLE_CODE: &str = "sandbox_unavailable";

/// Stable message emitted when optional confinement is unavailable.
pub const SANDBOX_UNAVAILABLE_MESSAGE: &str = "Sandbox unavailable; proceeding without confinement. Set sandbox.required = true to block execution.";

/// Stable diagnostic code for Sandbox unavailability caused by WSL1, which
/// cannot create the user namespaces bubblewrap needs.
pub const SANDBOX_WSL1_UNAVAILABLE_CODE: &str = "sandbox_wsl1_unavailable";

/// Stable message emitted when the Linux sandbox is unavailable because the
/// host is WSL1. Names the practical remedy (use WSL2) rather than the generic
/// unavailability message.
pub const SANDBOX_WSL1_UNAVAILABLE_MESSAGE: &str = "Sandbox unavailable: WSL1 cannot create the user namespaces bubblewrap needs. Use WSL2 for sandboxed commands.";

/// Stable diagnostic code for required Sandbox unavailability.
pub const SANDBOX_REQUIRED_UNAVAILABLE_CODE: &str = "sandbox_required_unavailable";

/// Stable message emitted when required confinement blocks execution.
pub const SANDBOX_REQUIRED_UNAVAILABLE_MESSAGE: &str =
    "Required Sandbox unavailable; command not executed.";
