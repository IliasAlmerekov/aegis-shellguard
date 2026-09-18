# ADR-037: A git Snapshot leaves the working tree untouched

## Status

Accepted. Sharpens the `Snapshot` and `Rollback` entries in `CONTEXT.md` and the
capture boundary of ADR-026 §5. Fixes issue #356.

## Context

`GitPlugin::snapshot` captured the working tree with one command:

```rust
git stash push --include-untracked -m aegis-snap-<timestamp>
```

`git stash push` is not a read. It writes the stash entry and then resets the
working tree to HEAD. Nothing put the work back. The only code path that
restored it was `rollback()`, which runs when a human asks for it, so on the
ordinary path, the command runs, it succeeds, nobody rolls back, the user's
uncommitted work stayed in the stash list and was gone from disk.

The blast radius is wider than `Danger`. `snapshots_required`
(`crates/aegis-policy/src/engine.rs`) returns `true` for any effect-opaque
command before it reads the risk level, and every interpreter-on-script
invocation is effect-opaque. So `python3 script.py`, assessed `Safe`, emptied
the working tree. Two details hid it: a clean tree short-circuits to
`CLEAN_SENTINEL` and touches nothing, so the bug only fired when there was work
to lose, and nothing in the output pointed at the stash entry holding it.

## Decision

### 1. `snapshot()` applies the entry it just created

After resolving the stash commit, the plugin runs
`git stash apply --index <hash>`. The working tree, the index, and untracked
files come back exactly as they were, and the entry stays in the stash list for
a later rollback. `git status --porcelain` is byte-identical before and after a
Snapshot, which is the regression test.

The tree is at HEAD when the apply runs, so it has nothing to merge against and
cannot conflict. If it fails anyway, the plugin returns the new
`SnapshotError::SnapshotNotRestored`, which names the stash hash and the exact
command that puts the work back. Failing loudly is correct here: the caller
counts the failure against Snapshot coverage (ADR-036) and the human is told
where their files are, instead of finding an empty tree and no explanation.

`git stash create` plus `git update-ref` was the other candidate. It never
touches the tree, but it does not capture untracked files, and untracked files
are most of what an agent has in flight.

### 2. `rollback()` parks the current state before it restores

Because the Snapshot no longer empties the tree, the work being rolled back is
still on disk when `rollback()` runs, and `git stash apply` refuses to overwrite
local changes. Rollback therefore runs
`git stash push --include-untracked -m aegis-pre-rollback-<timestamp>` first
when the tree is dirty, then applies the snapshot entry onto the clean tree.

This keeps the rollback contract of ADR-026 §5, restore the captured state,
without deleting anything to get there. The state a rollback replaces is
recoverable from its own stash entry, and the entry is logged with its hash.
`git reset --hard` plus `git clean -fd` would have produced the same tree and
destroyed everything created after the Snapshot, including output the human may
still want.

### 3. The stash entry is dropped by rollback or by retention, not by the success path

The entry outlives the command on purpose: it is the artifact `aegis rollback`
restores from. `rollback()` drops it after a successful apply. When `[prune]`
has `enabled = true`, Aegis applies its retention limits after recording a
Snapshot. The success path does not delete a new entry immediately, because
"the command succeeded" does not mean "nobody will want to undo it".

## Consequences

- A Snapshot is invisible in the working tree. An agent no longer loses work by
  running a script, and the guarded command runs against the tree the human
  approved it for.
- Rolling back can add one `aegis-pre-rollback-*` stash entry. It is logged, it
  is never dropped automatically, and it is the only trace of the state that the
  rollback replaced.
- `RollbackConflict` is now reached in a narrower case: the tree is clean by the
  time the apply runs, so a conflict means HEAD moved under the entry, not that
  the tree was dirty.
- Repositories with `core.autocrlf=true` get their line endings normalized on
  the round trip, because git rewrites files it checks out. The tests set
  `core.autocrlf=false` so their assertions do not depend on the host's global
  git config.
