# M3a — Disabled Toggle visibility

## Status

Closed 2026-08-17. M3b canonical hook wrapping was already closed.

Closure added two guards the implementation lacked: a behavioral parity contract
between the session-start notice and `aegis status`
(`tests/toggle_parity.rs`), and a documentation contract following the H9/M1
precedent. It also fixed a defect found by the live smoke rather than by the
suite — `aegis install-hooks` rejected the whole install when a third-party
`SessionStart`/`PreToolUse` entry omitted the optional `matcher`, which left the
operator with no notice at all.

## Finding

The global `Toggle` is an intentional operator control (ADR-005), but a persistent
`~/.aegis/disabled` file can leave wrapper and hook surfaces in unguarded
passthrough across sessions without a visible signal.

## Product boundary

Do not turn `aegis off` into a scanner-controlled command or remove the operator
escape hatch. The contract is explicit, observable unguarded passthrough:
toggle transitions are audited when possible, status is queryable, and a new
agent session cannot silently inherit disabled enforcement.

## Scope

- Keep `aegis off`, `on`, and `status` semantics and ADR-005 command-boundary
  sampling.
- Install and emit a disabled-state notice at session start for both Claude Code
  and Codex integrations.
- For command-level hook/JSON protocols, use only fields permitted by that
  protocol; never print stray stdout that invalidates JSON.
- If the disabled flag exists but CI forces enforcement, the session-start
  notice reports that effective enabled state rather than claiming passthrough.
- Both agent integrations use the protocol-valid `SessionStart` JSON envelope
  with `additionalContext` and no stray stderr. They register for
  `startup|resume`: effective enabled sessions retain routing guidance,
  effective disabled sessions emit the approved passthrough warning, and CI
  override sessions add the approved effective-state sentence.
- Do not add a wrapper text-mode warning per process or invocation; visibility
  is limited to the protocol-valid agent `SessionStart` surfaces.
- Preserve CI override behavior.
- Session-start notices do not create audit events; only `aegis off`/`on`
  transitions retain their existing best-effort audit contract.
- Existing installations receive the managed session hooks only after an
  explicit `aegis install-hooks` rerun; session runtime never self-updates
  installed hooks.

## TDD seams

1. Codex and Claude Code session-start output visibly reports disabled
   passthrough, or CI-forced enforcement when that override is active.
2. Pre-tool hooks remain valid JSON while disabled.
3. CI override keeps enforcement active and reports the effective state.
4. Toggle commands continue to append audit entries; an audit failure is loud but
   does not lie about the already-changed toggle state.

## Implementation sequence

1. Add failing agent-hook session-start tests.
2. Reuse `ToggleState`/`status_view` rather than reading the flag independently.
3. Add wrapper/JSON-safe visibility at the narrowest supported seams.
4. Update README, troubleshooting, and config/status docs.
5. Document that existing installations receive the new managed session hooks
   after an explicit `aegis install-hooks` rerun; do not self-update hooks at
   session runtime.

## Resolved product decisions

- Visibility is emitted for both `startup` and `resume` session events.
- The scope is limited to Claude Code and Codex `SessionStart` JSON envelopes;
  ordinary wrapper invocations receive no additional warning.
- Session-start visibility is informational, not an audit event. Toggle
  transitions remain the auditable action.
- Hook refresh remains an explicit operator action through `aegis install-hooks`.
- SessionStart hooks use standalone inline Toggle-state logic rather than
  sourcing the mutable managed helper, so they always retain one protocol-valid
  JSON response even if that helper is malformed or noisy.

## Verification

- `tests/agent_hooks.rs`, `tests/toggle_cli.rs`, `tests/full_pipeline_toggle.rs`,
  `tests/watch_mode.rs`
- `rtk cargo test --workspace`
- `rtk cargo clippy -- -D warnings`
- `rtk cargo fmt --check`
- `rtk cargo audit`
- `rtk cargo deny check`
