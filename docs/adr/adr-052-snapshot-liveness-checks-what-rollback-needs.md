# ADR-052: Snapshot liveness checks what Rollback needs, and only a question needs a terminal

## Status

Accepted. Sharpens [ADR-026](adr-026-snapshot-rollback-contract-for-1-0.md) §3
and supersedes its rule that Rollback asks before restoring an absent artifact.

## Context

ADR-026 §3 adds `aegis snapshot verify [<id>]` and a preflight in
`aegis rollback` that tells an absent artifact apart from an inconclusive check.
It leaves open what each provider checks, what a missing terminal blocks, and
what the commands return. Issue #215 builds both, and its triage and grill
settled these points.

Four facts in the code shaped the answers. The git provider restores with
`git stash apply` but first resolves the entry in `git stash list`, and fails
when the entry is gone, even if the stash commit still exists. Database
providers keep their dump or manifest in the local Snapshot store, so the
artifact is a file Aegis can stat without a network call, and their Rollback
fails with `RollbackDumpNotFound` when it is missing. The Docker provider stops
and removes the live container before it runs the recorded image, so a Rollback
against a missing image destroys the current container and creates nothing in
its place. The git provider records `clean` and the Docker provider records
`none` when there was nothing to capture, and Rollback treats both as a no-op.

ADR-026 asks the operator to confirm a restore against an absent artifact,
because a hard refusal would destroy the only recovery path on a false
negative. Under the decision below, every `Absent` is the same fact Rollback
itself checks before it acts, so no false negative exists: confirming would
either fail at once or, for Docker, destroy the current container.

ADR-026 also names "the database is unreachable" as an inconclusive example and
says the no-TTY rule mirrors `sandbox.required`. The first no longer holds under
the decision below, and ADR-029 removes `sandbox.required`.

## Decision

1. **Four outcomes.** A `Snapshot liveness` check returns `Present`, `Absent`,
   `Inconclusive` or `Nothing captured`. `Nothing captured` is the outcome for
   the git `clean` and Docker `none` records; it is not `Absent`.
2. **Each provider checks what its Rollback needs.**
   - Git: `Absent` when `git stash list` in the recorded directory has no entry
     for the recorded stash hash, or when that directory is gone. This is the
     same lookup Rollback performs, so the preflight cannot report `Present`
     for a Snapshot Rollback would refuse.
   - Docker: `Absent` when any recorded image no longer exists; `Inconclusive`
     when the daemon does not answer.
   - PostgreSQL, MySQL/MariaDB, SQLite: only the local dump in the Snapshot
     store is checked, with no network access.
   - Supabase: the manifest is read and its eligibility recomputed, then the
     dump it names is checked, all locally.
   - For the database providers, `Inconclusive` covers a read error. An
     unreachable server surfaces as an ordinary Rollback error.
3. **Every check is bounded at 5 seconds**, a constant. A check that runs out of
   time is `Inconclusive`.
4. **`SnapshotPlugin` gains a required liveness method.** All six providers
   implement it; a default would let a provider answer "unknown" by omission.
5. **`aegis snapshot verify`** checks the live records of the Snapshot ledger
   (ADR-049) one after another, or the one id given. It prints text, or JSON
   with `--format json`. It exits `0` when every Snapshot is `Present` or
   `Nothing captured`, `1` when any is `Absent`, `5` when any is `Inconclusive`
   and none is `Absent`, and `4` for an unknown or pruned id or an error.
   Exit code `5` joins the public exit-code table; `2` keeps its single
   meaning, Denied.
6. **`aegis rollback` runs the same check before it writes anything.** On
   `Present` or `Nothing captured` it restores without asking. On `Absent` it
   refuses without asking: it says the artifact is confirmed gone and exits
   `1`, the code `verify` gives for `Absent`. `--yes` does not override this.
   On `Inconclusive` it says the check could not complete and asks; `y` or
   `yes` attempts the restore anyway, any other input refuses with exit `2`.
7. **A missing terminal blocks only a question.** Without a TTY,
   `Inconclusive` fails closed with exit `2` and an explicit message unless
   `--yes` is given. `Present` and `Nothing captured` need no terminal, so an
   agent hook or `aegis rollback --last` (#499) can restore a live Snapshot.
   `--yes` also skips the question when a TTY is present.
8. **The preflight outcome is not written to the Audit log.** The entry
   `aegis rollback` writes keeps the exact shape ADR-049 matches.
9. **A record Rollback cannot use is an error, not an outcome.** A malformed
   `snapshot_id`, an artifact path outside the Snapshot store, or a Supabase
   manifest that fails its eligibility check makes the liveness check return
   an error. `verify` shows the reason on that row and exits `4`; `rollback`
   exits `4` without asking.

## Consequences

- The git preflight reports `Absent` after `git stash drop`, although the stash
  commit stays until garbage collection. Restoring by hash without the entry
  would need a change to the Rollback path itself and is not part of this
  decision. If that change lands, git `Absent` must be redefined before any
  prompt returns.
- A Docker Rollback can no longer remove the live container when its image is
  gone.
- A database Snapshot can pass the preflight and still fail to restore when its
  server is down; the operator then sees the Rollback error.
- Scripts can tell `Absent` (`1`) from `Inconclusive` (`5`) without parsing
  output, and `aegis rollback` uses `1` for the same `Absent`.
- With a silent Docker daemon, `verify` over many Docker records takes up to
  5 seconds per record.
- A tampered record that points outside the Snapshot store shows up in `verify`
  as an error instead of hiding behind `Absent`.
- PRD §5.4 states the no-TTY rule with its condition.

## Alternatives considered

- **Ask before restoring an `Absent` Snapshot, as ADR-026 §3 says.** Keeps a
  path open for a false negative, but none exists when the check mirrors
  Rollback: git and the database providers fail at once, and Docker removes the
  live container before `docker run` fails.
- **Ask on `Absent` for every provider except Docker.** Two rules for one
  outcome, and the question stays empty for the rest.
- **Git `Absent` only when the stash commit object is gone.** Closer to what
  `git stash apply <hash>` can do, but Rollback refuses without the list entry,
  so the preflight would report `Present` for a Snapshot that cannot be
  restored.
- **Probe the database server during the preflight.** Matches ADR-026's
  example, but puts a connection and a timeout on every Snapshot `verify`
  lists, for a fact Rollback discovers anyway.
- **Report a record Rollback cannot use as `Absent`, or as a fifth outcome.**
  `Absent` would misstate the cause and hide a tampered path; a fifth outcome
  adds a term and a type for a rare case an error already covers.
- **Fail closed without a TTY on every Rollback.** Matches PRD §5.4 word for
  word, but blocks every agent-driven Rollback of a live Snapshot unless the
  agent passes `--yes`, which would then be passed by habit.
- **Exit `2` for `Inconclusive` in `verify`, or for `Absent` in `rollback`.**
  Gives `2` a second meaning in a public table where it means Denied.
- **Record the preflight outcome in the Audit entry.** Changes the Audit schema
  and the entry shape ADR-049 depends on; left for a separate change.
