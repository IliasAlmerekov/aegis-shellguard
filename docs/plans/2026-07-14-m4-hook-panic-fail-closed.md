# M4 — Hook panic containment

## Status

Accepted — implemented via TDD (ADR-023).

## Finding

The hook entry point converts normal parse/validation errors into deny JSON, but
an unwind can terminate before any protocol response is emitted. Agent clients
may interpret missing output as permission to continue.

## Scope

- Put `catch_unwind` at the outer Rust hook boundary, not around individual
  helpers.
- Convert a panic payload into the existing agent-compatible deny shape without
  exposing sensitive internals.
- Keep the panic hook/logging behavior deterministic and avoid double-printing
  protocol output.
- Do not use panics for expected hook errors; existing typed paths remain primary.
- **Script-level layer (added at implementation):** the installed per-agent
  `Hook` scripts stop `exec`-ing the binary, capture its stdout and exit status,
  and emit their own deny response on a non-zero exit status — covering the
  failure classes an in-process unwind guard structurally cannot (`abort`,
  SIGSEGV, OOM-kill). Empty stdout with exit 0 stays a legitimate noop.

## TDD seams

- Inject a test-only panic behind the public hook dispatch seam and assert valid
  deny JSON with exit behavior expected by Claude/Codex.
- Ordinary allow/noop/deny inputs remain byte/structure compatible.
- A non-string panic payload still produces a stable generic reason.
- Script seam: point each installed `Hook` script at a stub binary that exits
  non-zero without output and assert the script's own deny response and exit 0;
  a companion test pins that a stub exiting 0 with no output stays silent.

## Implementation sequence

1. Add one failing boundary-panic integration test.
2. Wrap dispatch with `AssertUnwindSafe` only if the captured inputs require it;
   document why.
3. Reuse `hook_deny_output` and existing render/exit flow.
4. Add parity coverage for both installed hook shims.
5. Stop `exec`-ing the binary in both `Hook` scripts; capture stdout and exit
   status and deny on a non-zero exit status.
6. Add script-level parity tests (one per agent) plus the noop-contract pin.

## Verification

- Focused hook tests
- `rtk cargo test --workspace`
- `rtk cargo clippy -- -D warnings`
- `rtk cargo fmt --check`
- `rtk cargo audit`
- `rtk cargo deny check`
