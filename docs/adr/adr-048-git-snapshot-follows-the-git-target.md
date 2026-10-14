# ADR-048: The git Snapshot follows the Git target; an unresolved Git target is disclosed, never covered by a cwd Snapshot

## Status

Accepted.

## Context

Snapshot creation passed the process working directory (or `.` when it was
unavailable) to every `Snapshot plugin`. Since ADR-041, `git -C ../other
filter-branch` and `git --git-dir=../other/.git --work-tree=../other
filter-branch` prompt, but the git Snapshot they took covered the cwd
repository. The repository the command changes got no Snapshot, while the
dialog and the audit entry reported coverage that did not exist. ADR-041
Consequences deferred this gap.

## Decision

Planning resolves a **Git target** (see `CONTEXT.md`) for every effective `git`
stage that is `Danger` and redirects its working tree. The scanner reports the
raw `-C`, `--work-tree` and `--git-dir` values per reading of the git global
options (the ADR-041 walk, now also yielding option values). Planning resolves
them only when a recovery backstop applies, so the safe path pays nothing.

- `-C` options apply in order; a relative path resolves against the previous
  result, starting at the process cwd. An empty `-C` value changes nothing.
- `--work-tree` applies after the `-C` chain, and only together with
  `--git-dir`. Git finds the repository from the `-C` directory when no
  `--git-dir` is given, so a lone `--work-tree` does not name the tree at risk.
- `--git-dir` is accepted only when it canonicalizes to `<work tree>/.git`.
- Everything else is unresolved: `--git-dir` alone or elsewhere, `--work-tree`
  alone, a path with `$`, a glob, a brace expansion, a backslash or `~`,
  `GIT_DIR=` or `GIT_WORK_TREE=` in the stage's env prefix, a launcher that
  changes the directory before git runs (`env -C`, `env --chdir`, `sudo -D`,
  `sudo --chdir`), a missing path, a relative path with no cwd, and an unlisted
  option whose two readings give different targets. A process-environment
  `GIT_DIR` is not a redirect: only the env prefix of the stage is read.

Which Snapshots run, as `SnapshotPlan::Required { git_targets }` that execution
follows without deriving anything from the command:

- Only the git plugin follows the Git target. Docker, Postgres, MySQL, Supabase
  and SQLite keep using the process cwd.
- Every resolved Git target gets its own git Snapshot.
- The process cwd gets a git Snapshot only when some part of the command needs
  one without a git redirect (`git -C a ...; rm -rf x`, an effect-opaque
  command).
- An unresolved Git target never gets a substitute cwd Snapshot, and Aegis does
  not try "both directories". The plan stays `Required` even if policy asked
  for no Snapshot, so the gap is disclosed.

Disclosure. A confirmed (human) run still prompts; the dialog and the audit
entry name each Git target or the reason it is unresolved. An unattended
auto-approved run outside `Mode::Audit` halts with the Recovery obligation
unmet (`RecoveryDegradation::UnresolvedGitTarget`), even when other Snapshots
of the same command succeeded.

Rollback needs no change: the git `snapshot_id` encodes the directory the plugin
ran in, so the Snapshot's location is the Git target.

## Consequences

`git -C <other> <Danger command>` restores `<other>`'s uncommitted work and
leaves the cwd repository untouched. Several redirected stages yield several
Snapshots.

An effect-opaque command is still given the ADR-016 recovery Snapshot of the
process cwd. `git -C "$X" filter-branch` is effect-opaque (ADR-043) and also
has an unresolved Git target, so it gets a cwd Snapshot for the opacity while
the Git target stays disclosed as uncovered.

Whether the cwd also needs a git Snapshot is decided by comparing the `Danger`
pattern ids of redirected stages with those of plain stages and with the
assessment's matches. This is an approximation: a stage whose env prefix is
dropped by normalization can look like a plain stage and add a cwd Snapshot
that was not strictly needed. The extra Snapshot is harmless.

`git reset --hard` (GIT-001) is `Warn`, so policy requires no Snapshot for it,
with or without `-C`. This ADR changes no risk level.

Out of scope: targeting after `cd`, a `--git-dir` outside `<work tree>/.git`,
redirecting non-git plugins, and the general `.` fallback for an unavailable
cwd (#220).
