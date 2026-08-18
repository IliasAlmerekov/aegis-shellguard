# ADR-026 — The 1.0 Snapshot/Rollback contract: six mandatory providers, copyable ids, verified recovery

## Status

Accepted

## Context

[ADR-004](adr-004-snapshots-are-best-effort-audit-is-append-only.md) established
that a `Snapshot` is best-effort. That remains true about *coverage* — no
provider is universal, and a `Snapshot` captures pre-execution state rather than
undoing a command's effects. What ADR-004 does not settle is the **release
meaning** of the subsystem, and 1.0 raises the stakes: with language-aware
analysis deferred to 1.x ([ADR-024](adr-024-language-aware-analysis-ships-opt-in-and-is-not-a-1-0-release-gate.md)),
the rule corpus, confinement, and Snapshot/Rollback are what 1.0 actually
promises. Snapshot is the stated differentiator between Aegis and a plain
allow/deny hook.

Three gaps sit between that promise and the code.

**Ids do not round-trip.** `snapshot_id` is a composite string joined by a tab
(`const SEP: char = '\t'` in `git.rs`, `sqlite.rs`, `mysql/mod.rs`,
`postgres/mod.rs`). In `aegis snapshot list` a tab renders as column padding, so
the listed id cannot be selected and pasted as the single argument to
`aegis rollback`. The recovery path documented in PRD §5.4 is therefore not
operable by the human it exists for. This is finding M9.

**Coverage claims exceed the contract.** `README.md:5` reads "Make AI agents ask
first. Undo them when they don't." A `Snapshot` is a pre-execution capture by an
applicable provider; it is not an undo of the command, and where no provider
applies there is nothing to restore. This is finding M8.

**Degradation is silent.** When no provider is applicable, nothing in the
confirmation dialog tells the operator that this particular approval is
unrecoverable. The dialog's own default is already safe — `confirm_screen.rs`
approves only on `y`/`yes` and treats every other input, including a bare Enter,
as `Deny` — but a human who has typed `y` fifty times does not learn from a
default they never exercise.

The remaining question is what "mandatory" means for the six providers PRD §5.4
names (Git, Docker, PostgreSQL, MySQL/MariaDB, SQLite, Supabase). Today the
evidence is uneven: CI runs one live Docker snapshot/rollback test, Postgres and
MySQL are exercised without live servers, and `supabase/` carries no test module
at all. Declaring a provider mandatory without a repeatable proof reproduces
exactly the document/code divergence this release work exists to remove.

## Decision

### 1. All six providers are mandatory for 1.0, and mandatory means proven in CI

Git, Docker, PostgreSQL, MySQL/MariaDB, SQLite, and Supabase are all part of the
1.0 promise. Each must have a repeatable snapshot → rollback cycle exercised in
CI: Postgres and MySQL via service containers, Docker via the existing live test,
Git and SQLite locally.

**Supabase is proven through its Postgres compatibility** — the same container,
the same code path — not through a `supabase start` stack in CI. That stack is a
multi-container Compose environment whose startup cost and flakiness would be
paid on every run to re-prove a path already covered. One live Supabase run
against a real project is performed once before release and recorded as evidence
in `docs/release-readiness.md`; it is release evidence, not a repeating gate.

### 2. `snapshot_id` uses `:` as its separator, at format version `v3`

The tab separator is replaced by `:`. Both id components are hex-encoded, so no
collision is possible; `:` needs no shell quoting, unlike `~`, and does not blur
into hex digits, unlike `.`. The version prefix moves from `v2` to `v3` so the
format is identified explicitly rather than inferred from which separator a
parser happens to find.

**Legacy `v2` tab-joined ids remain parseable forever.** The audit log is
append-only and is a v1 public contract; an id recorded before this change must
stay recoverable after it.

`aegis snapshot list` additionally prints a ready-to-use `aegis rollback '<id>'`
line per row. `list` stays cheap: it reports *recorded* snapshots and hides
pruned ones, and it does not check whether artifacts still exist. Contacting six
providers — several over the network — would turn an instant command into a
sequence of timeouts.

### 3. Liveness is checked by `aegis snapshot verify`, and by `rollback` before it acts

A new `aegis snapshot verify [<id>]` reports, per snapshot, whether the artifact
still exists: a Git stash ref, a Docker image, a dump file for
SQLite/PostgreSQL/MySQL/Supabase. Results are not cached — a cached liveness
column is a second source of truth about state Aegis does not own.

`aegis rollback` performs the same check itself before restoring. Two outcomes
are distinguished, because conflating them misinforms at the worst moment:

- **Artifact absent** (the ref or file is confirmed gone) — the operator is told
  plainly that the artifact was removed and that restoration will almost
  certainly fail, and is asked to confirm.
- **Check inconclusive** (the database is unreachable, the Docker daemon does not
  answer) — the operator is told the check could not be completed, and is asked
  to confirm.

In both cases the default answer is refusal, and confirming means "attempt the
restore anyway". A hard refusal on absence was rejected: a false negative would
then destroy the operator's only recovery path.

**Without a TTY, `aegis rollback` cannot ask, so it fails closed** with a
non-zero exit and an explicit message; `--yes` supplies the confirmation. This
mirrors `sandbox.required`. Treating a missing TTY as consent would disable the
warning exactly where no human is present to receive it.

### 4. An inapplicable provider is disclosed in red, and approval requires the full word

When no provider applies to a `Danger` command, the confirmation dialog states in
red that the command cannot be rolled back, and approval requires the full word
`yes` — the shorthand `y` is rejected for this case only. The same text is
written to Shell stderr and to the audit entry without colour, so the signal
survives `NO_COLOR` and non-TTY surfaces.

A dedicated word (`no-rollback`) or retyping the command was rejected: both are
copied without being read, adding friction without adding attention.
A `snapshot.required` hard block is **not** 1.0 — it would make every non-Git
directory unusable.

### 5. Wording: `Snapshot` captures, `Rollback` restores that capture

`README.md`'s "Undo them when they don't" is replaced by wording that promises
restoration of what was captured, not undo of what was done. `CONTEXT.md`, the
TUI/explanation copy, the threat model, and the examples say the same thing:
a `Snapshot` is a pre-execution capture taken by an applicable provider; a
`Rollback` restores that capture. Keeping the slogan with a disclaimer beneath it
was rejected — the slogan is what gets quoted.

## Consequences

- PRD §5.4 becomes a stronger promise than it is today and must state the
  provider tiering, the id format, `verify`, and the disclosure rule.
- The 1.0 CI cost rises by two service containers (Postgres, MySQL).
- M8 and M9 are both 1.0 blockers; their acceptance criteria in `TASKS.md` widen
  to match this ADR.
- One live Supabase run becomes a release-readiness prerequisite.
- The `v2` parser is permanent maintenance surface.

## Alternatives considered

- **Git + SQLite mandatory, the rest best-effort.** Cheapest to prove, but it
  withdraws four providers PRD §5.4 already promises and weakens the
  differentiator that 1.0 rests on after ADR-024.
- **`list` verifies every row.** Honest, but it makes the discovery command as
  slow and failure-prone as the providers it queries.
- **Liveness status cached on the listing.** Requires storage Aegis would then
  have to keep true; a stale "available" is worse than no column.
