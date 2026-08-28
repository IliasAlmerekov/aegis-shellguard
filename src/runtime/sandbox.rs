//! Stable active-channel diagnostics for optional Sandbox execution.

/// Stable diagnostic code for optional Sandbox degradation.
pub const SANDBOX_UNAVAILABLE_CODE: &str = "sandbox_unavailable";

/// Stable message emitted when optional confinement is unavailable.
///
/// Names no config remedy: `sandbox.required` left the config contract in
/// #240 and is now accepted-but-ignored, so telling a user to set it would be
/// stale, misleading advice.
pub const SANDBOX_UNAVAILABLE_MESSAGE: &str =
    "Sandbox unavailable; proceeding without confinement.";

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
