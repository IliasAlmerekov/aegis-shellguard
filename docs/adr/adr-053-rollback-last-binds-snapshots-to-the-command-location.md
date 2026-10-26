# ADR-053: `aegis rollback --last` binds Snapshots to the Command location and asks before it restores

## Status

Accepted.

## Context

`aegis rollback` took one required `snapshot_id`, which the operator had to dig
out of the Audit log. The rollback-first promise needs a shortcut, and the
shortcut must not restore the wrong thing. Two agents may work on one machine:
one deletes a directory in repo A, another runs `docker rm` in project B a
minute later. Running `aegis rollback --last` in repo A must restore repo A and
leave the container alone.

Two facts in the code blocked a plain "latest Snapshot" lookup. Only watch-mode
entries recorded `cwd`; shell-wrapper entries, the main mode, recorded no
location. And `AuditSnapshot` held only `plugin` and `snapshot_id`: nothing
recorded which working tree a git Snapshot covered, although the git plugin
already resolves a Git target (ADR-048, ADR-050). Docker Snapshot scoping
ignores cwd by design (ADR-039), and database Snapshot targets have no
directory.

## Decision

1. **Command location.** Every new `AuditEntry` records the `cwd` it ran in,
   in shell-wrapper and watch mode, under the existing field name `cwd`. A
   watch entry keeps the frame's `cwd`, which may be absent. Every git
   `AuditSnapshot` records its resolved Git target in a new optional field,
   `git_target`: the canonical top level of the working tree the plugin ran in.
   The git plugin sets it through `SnapshotPlugin::snapshot_target`, whose
   default returns `None`.
2. **Old lines are unchanged.** Both fields are optional and skipped when
   absent. The integrity payload serializes `snapshots` and the entry as before,
   so a legacy line hashes to the same value, and a test pins two pre-change
   hashes. A recorded `cwd` and `git_target` are covered by the chain of new
   entries.
3. **Bound Snapshot.** A Snapshot is bound to the current location, the
   directory `aegis rollback --last` runs in, when:
   - git: its `git_target` equals the top level of the git repository that
     contains the current directory. Outside a git repository, when the entry's
     `cwd` equals the current directory or lies beneath it;
   - sqlite: its database file lies under that repository top level, or under
     the current directory outside git;
   - docker, postgres, mysql, supabase: never.
4. **Candidate.** A candidate is a command with a recorded `cwd` and at least
   one live, bound Snapshot. An entry without `cwd` is never a candidate; its
   Snapshots stay reachable through `aegis rollback <id>`. `Rollback` entries
   never count, as the Snapshot ledger already ignores them as creations.
5. **Selection reads the Snapshot ledger.** `SnapshotLedger::recent_commands`
   returns the commands that still have a live Snapshot, latest first in log
   order (ADR-049 §6), each with its `cwd`, `source`, `git_target`s and last
   restore time. `--last` does not parse the Audit log a second time.
6. **Confirmation, not a time window.** `--last` picks the latest candidate.
   On a terminal it prints the command, time, `source`, the bound Snapshots it
   will restore and the unbound ones it skips with their `aegis rollback <id>`
   line, then asks yes / no / list, defaulting to no. "list" shows recent
   candidates for the location and lets the operator pick one. Without a
   terminal and without `--yes` it prints the same summary and list and exits
   `2`, the fail-closed rule of ADR-052 §7. With `--yes` it restores the bound
   Snapshots of the latest candidate and never an unbound one. A command that
   was already restored is still selected, and the summary says when.
7. **Restore reuses the #215 preflight.** Each bound Snapshot goes through the
   liveness check and `--yes` rules of ADR-052. A failing provider does not stop
   the others. The command exits with the code of the first failure, and an
   `aegis rollback` Audit entry is written only for Snapshots that restored.
8. **No match** exits `4` with a message that names the location and points to
   `aegis rollback <snapshot_id>` and `aegis audit`.

## Consequences

- A git Snapshot of a repository subdirectory is bound by the repository top
  level, not by the subdirectory it ran in.
- An entry written before this change has no `cwd`, so `--last` skips it. A
  watch entry from before has a `cwd` but no `git_target`, so inside a git
  repository its git Snapshot is not bound.
- Only an approved or auto-approved command is a candidate. A denied entry
  can carry Snapshots from the recovery-degradation path, but the command never
  ran, so there is nothing to undo.
- A pre-499 binary reads a new shell-wrapper audit line that carries `cwd` as
  a watch entry. That breaks its integrity chain, so a downgrade past this
  change fails the integrity check on the log.
- A git Snapshot with nothing to capture (`clean`) still carries a target, so a
  later no-op command can become the latest candidate. Its restore is a no-op.
- The restore order follows the order the Snapshots were taken. A git restore
  parks untracked files, so a sqlite database file inside the repository that
  git does not ignore is removed before its sqlite restore runs. Ignore the
  file in git, or restore it with `aegis rollback <id>`.
- Repeated `--last` does not step back through older commands: there is no undo
  stack.

## Alternatives considered

- **A time window for ambiguity.** Hides the choice behind a threshold that is
  wrong for slow and fast agents alike. A confirmation shows the choice instead.
- **Bind git Snapshots by `cwd` alone.** A command run from a subdirectory or
  with `git -C` would bind to the wrong repository.
- **Bind docker Snapshots by `cwd`.** ADR-039 rejects it: the container set does
  not depend on the directory.
- **Parse the Audit log in the `--last` code.** A second reader would drift from
  the ledger's prune and rollback rules (ADR-049).
