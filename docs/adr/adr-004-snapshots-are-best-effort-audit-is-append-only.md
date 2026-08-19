# ADR-004 — Snapshots are best-effort; audit is append-only

## Status

Accepted. Partially superseded by
[ADR-031](adr-031-unattended-destructive-execution-requires-recovery.md) for
`Unattended destructive execution`: there a `Ready` Recovery is required rather
than best-effort. The Audit integrity decisions below remain in force.

## Decision

Recovery and forensics are important, but they are not symmetric guarantees.

## Current contract

- snapshots are provider-based and best-effort
- rollback may still fail or conflict
- audit output remains append-only JSONL
- the optional audit integrity chain detects corruption and inconsistent edits;
  it has no keyed or external anchor

## Implication

- docs must describe rollback honestly
- audit integrity claims must stay tied to the configured integrity mode
