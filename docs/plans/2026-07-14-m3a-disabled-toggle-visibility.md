# M3a — Disabled Toggle visibility

## Status

Draft — requires a finding-specific `grill-with-docs` session before TDD. M3b
canonical hook wrapping is already closed.

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
- Decide whether wrapper text mode warns once per process or per invocation;
  JSON mode must remain structurally valid.
- Preserve CI override behavior.
- Session-start notices do not create audit events; only `aegis off`/`on`
  transitions retain their existing best-effort audit contract.

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

## Verification

- `tests/agent_hooks.rs`, `tests/toggle_cli.rs`, `tests/full_pipeline_toggle.rs`,
  `tests/watch_mode.rs`
- `rtk cargo test --workspace`
- `rtk cargo clippy -- -D warnings`
- `rtk cargo fmt --check`
- `rtk cargo audit`
- `rtk cargo deny check`
