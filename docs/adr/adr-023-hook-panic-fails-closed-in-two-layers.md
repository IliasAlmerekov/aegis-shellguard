# ADR-023 — A contained Hook panic fails closed in two layers

## Status

Accepted

## Context

The Aegis `Hook` communicates its decision to a coding agent purely through a
structured JSON response on stdout: a deny carries a top-level `reason` plus a
`hookSpecificOutput` block with `permissionDecision: "deny"`. Every *expected*
failure already produces that deny shape, but an *unexpected* failure does not.
If the Rust process unwinds on a panic anywhere across the `Hook` entry point,
it dies before printing anything; the agent sees empty stdout and may interpret
"no structured decision" as "nothing objected", so the command runs unscanned.
The same hole is open one level down: the installed per-agent `Hook` script ends
with `exec` of the Aegis binary, so once the binary is `abort`ed, killed by a
stack-overflow SIGSEGV, or OOM-killed, no process is left to speak on Aegis'
behalf.

The one moment a guardrail must be loudest — when it is itself broken — is the
moment it goes silent, and silence is read as consent.

## Decision

Two independent layers turn abnormal termination of the `Hook` into the
ordinary deny shape the agent already understands.

**Layer 1 — inside the binary.** The `Hook` entry function is the single place a
catch is installed. `std::panic::catch_unwind` wraps only the call that reads
stdin and produces the hook outcome; rendering and writing the response happen
outside the guard. On unwind the outcome is the existing deny variant built by
the existing deny-output constructor, with one fixed, detail-free reason
(`aegis hook failed internally; refusing to run command unscanned`) used
identically for `&str`, `String`, and non-string payloads. The payload is never
interpolated into the response. A minimal panic hook is installed at the top of
the `Hook` entry function and is not restored; it prints one deterministic
stderr line (`aegis: internal hook panic contained`) and appends payload and
location only when the user opts in through `RUST_BACKTRACE` or `AEGIS_DEBUG`.
Scope is `Hook` mode only — no panic hook is installed in `main`, so the
shell-proxy, `watch`, and TUI paths keep the full default panic output. Response
emission moves off the panicking print macro to an explicit locked-stdout write
plus flush; a write error is ignored silently and does not alter the exit code.
The `Hook` exit code stays 0 for allow, noop, ordinary deny, and contained panic
alike — with these agent clients only exit 0 gets the JSON decision parsed, so a
non-zero exit would demote a deny into a non-blocking hook error. No audit entry
is written for a contained panic (there is no assessment at that point), and no
`tracing` event is emitted (the binary initializes no subscriber).

**Layer 2 — inside the installed per-agent `Hook` scripts.** Both per-agent
`PreToolUse` scripts stop `exec`-ing the Aegis binary. They capture its stdout
into a variable and record its exit status. Abnormal termination is defined as
**non-zero exit status only**; empty stdout with exit 0 stays a legitimate noop
and is forwarded as silence. On abnormal termination the script prints its own
deny response with the reason `aegis hook terminated abnormally; refusing to run
command unscanned` — deliberately distinct from the existing `aegis binary
unavailable...` reason — and exits 0, symmetric to the existing binary-unavailable
fail-closed path. No JSON validation happens in the scripts (that would require
an external tool such as `jq` as a new runtime dependency of a security-critical
path). Double-printing is structurally impossible: a contained unwind exits 0
with the deny JSON, which the script merely forwards; an abnormal termination
produces no stdout, and only then does the script speak. `Toggle`/CI-override
handling, the shared toggle-state helper sourcing, and the binary-unavailable
branch are unchanged and still short-circuit before the binary runs. The
registered command in the settings files is unchanged — only script *content*
changes, so existing installations are repaired by the idempotent installer,
which already rewrites on content mismatch.

## Consequences

- A contained panic or an abnormal binary termination now reaches the agent as a
  normal deny, never as silence.
- The two layers are complementary, not redundant: the in-process layer is the
  only one that can give a *specific* reason, and the script layer is the only
  one that survives the binary's death. Shipping only one leaves a real hole.
- Replacing `exec` with a captured invocation costs one extra process per hook
  call. This is the hook path, not the scanner hot path, so the sub-2 ms
  safe-path budget is unaffected.
- The panic reason is fixed and detail-free; the two script-level reasons are
  distinct, which is a diagnosability decision as much as a security one.
- **Non-goals, stated honestly:** external SIGKILL, an OOM-kill of the agent
  process itself, and a corrupted `Hook` script are not covered — the only
  signal the outer layer has is a non-zero exit status. The shell-proxy path
  already fails closed structurally (a panic there exits non-zero and the
  wrapped command never executes); the `watch` NDJSON path has a different
  streaming contract and needs its own analysis.
- No audit entry, no new decision kind, no change to the deny response shape,
  and no `tracing` subscriber are introduced. Panics remain something Aegis
  never uses for expected error handling.
