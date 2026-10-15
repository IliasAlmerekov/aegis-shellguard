# ADR-049: One Snapshot ledger reads the Audit log for list, prune and Rollback

## Status

Accepted.

## Context

Three readers answered "which Snapshots exist" from the Audit log, each with
its own rules. `aegis snapshot list` keyed rows by `snapshot_id` and hid every
pair sharing a pruned id. `aegis rollback` did the same. Retention in
`aegis-snapshot` parsed `~/.aegis/audit.jsonl` with its own minimal types. It
ignored `AEGIS_AUDIT_PATH` and rotated archives, skipped unreadable lines, and
let an `aegis rollback` entry refresh a Snapshot's age. So pruning `git:X` hid
`docker:X`. A prune entry in `audit.jsonl.1.gz` counted for list and Rollback
but not for prune. A corrupt line made list and Rollback fail while prune went
on deleting. Issue #499 (`aegis rollback --last`) needs one answer to build on.

## Decision

`aegis_audit::SnapshotLedger` is the one reading of the Audit log that answers
which Snapshots exist, which were pruned, and which record a `snapshot_id`
resolves to. `aegis snapshot list`, `aegis snapshot prune`, `aegis rollback`
and automatic prune all go through it. `aegis-snapshot` no longer parses the
Audit log; `SnapshotRegistry::resolve_prunable_records` takes the ledger's live
records (`aegis_types::RecordedSnapshot`) as input. `aegis-snapshot` still does
not depend on `aegis-audit`.

The ledger follows these rules.

1. The key of a Snapshot is the pair (plugin, `snapshot_id`). A prune hides
   only the pair its `snapshots` entry names. A legacy prune entry with an
   empty `snapshots` falls back to the id in its `aegis prune <id>` command and
   hides every pair with that id, because it names no plugin.
2. A prune is final. A later record of a pruned pair does not bring it back,
   because prune deleted the Snapshot artifact.
3. Only creation entries record a Snapshot. The `Approved` entry that
   `aegis rollback <id>` writes neither creates nor refreshes a record, so
   `recorded_at` is the creation time. The ledger matches that entry by its
   exact shape: risk `Safe`, one snapshot, and the command
   `aegis rollback <that snapshot's id>`. A guarded command that only starts
   with `aegis rollback ` still records the Snapshot taken for it. A Rollback does not extend a Snapshot's
   life under the age and count retention rules.
4. The ledger reads through `AuditLogger::from_audit_config`. It honours
   `AEGIS_AUDIT_PATH`, reads the rotated `audit.jsonl.N` and
   `audit.jsonl.N.gz` segments, and takes the shared lock.
5. An unreadable audit line makes loading the ledger fail. `snapshot list`,
   `snapshot prune` and `rollback` exit with that error and delete or restore
   nothing. Automatic prune logs a warning and prunes nothing. Every failure
   lands on the side that keeps data.
6. "Latest" means latest in log order: segments oldest to newest, then line
   order. `snapshot list` shows rows newest first in that order. `recorded_at`
   stays the entry timestamp and feeds only the age rule, so clock skew cannot
   reorder records or change what Rollback picks.
7. `resolve(id)` returns the latest live record with that id across plugins.
   When every record with that id is pruned, Rollback fails with
   `SnapshotPruned`. When no record has that id, it fails with
   `SnapshotIdNotFound`.

## Consequences

- `snapshot list` shows one row per (plugin, id) pair. Before, it showed one
  row per id.
- Rollback still does not check that a Snapshot artifact exists before it
  restores. ADR-026 asks for that check; issue #215 tracks it.
- A Snapshot rolled back after its age limit is now pruned at the next prune.
  Before, the rollback entry kept it alive.
- A corrupt Audit log line now stops manual prune instead of being skipped.
  The operator has to repair the line before prune runs again.
- The Audit log format and the `AuditSnapshot` fields do not change, so a
  revert needs no data migration.
