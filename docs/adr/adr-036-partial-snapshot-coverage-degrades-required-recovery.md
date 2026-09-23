# ADR-036 — Partial Snapshot coverage degrades Required recovery

## Status

Accepted. Reverses one sentence of the `Recovery degradation` glossary entry in
`CONTEXT.md`, extends ADR-016, and supersedes ADR-031 §8's `RecoveryStatus` rule
and §9 for the case where an applicable plugin produces no record at all.

## Context

`recovery_status` (`src/runtime/recovery.rs`) resolved the ADR-016 Required
recovery obligation from the snapshot record count alone, and only an empty set
degraded it:

```rust
Some(if snapshots.is_empty() {
    RecoveryStatus::Degraded(RecoveryDegradation::NoSnapshotAvailable)
} else {
    RecoveryStatus::Ready
})
```

`SnapshotRegistry::snapshot_all` calls every applicable plugin, logs a warning
when one fails, and returns the records that succeeded. So when git succeeded
and sqlite failed, one record came back, `recovery_status` said `Ready`, and the
barrier never fired. Half the coverage was missing and the human saw nothing
about it at the one moment they were deciding.

`CONTEXT.md` did not merely omit this case. It ruled it out: "It is never
recorded while `Recovery status` is `Ready` — a partial failure alongside a
usable `Snapshot` is a per-attempt fact, not a degradation of the obligation."
That was a recorded decision, so issue #312 asked for a decision before code.

The case for keeping it: plugins cover different state. If the git plugin
succeeds and the docker plugin fails while the command only touches files,
coverage is formally incomplete and practically sufficient, and prompting is a
false positive. False positives in a guardrail teach people to bypass it.

## Decision

Partial coverage degrades the obligation. `Recovery status` is `Ready` only when
every applicable plugin produced a record.

The argument that decides it: Aegis does not know what a command will touch.
Required recovery exists precisely for commands whose effect cannot be read off
the command text — that is what effect-opaque means. "The docker failure does
not matter here" is a judgment about the command's eventual effect, and the code
making that judgment would be guessing. The human in front of the prompt can
make it; `recovery_status` cannot. Reporting `Ready` when a provider failed
tells the human something false, and it does so in the one place where they are
choosing whether to accept the risk. Silently overstating coverage is worse than
a prompt they can dismiss in one keystroke, because it removes the choice
instead of costing them a moment.

Three pieces make this work.

**One pass produces both numbers.** `SnapshotRegistry::snapshot_all` now returns
`SnapshotCoverage { records, applicable }`, where `applicable` counts the
plugins that reported themselves applicable during that same pass.
`SnapshotRegistry::applicable_plugins` still exists for the planning stage,
which runs before the confirmation dialog, but `recovery_status` does not use
it. Applicability is live state: `DockerPlugin::is_applicable` asks the daemon
what is running (ADR-039), and `GitPlugin::is_applicable` fails open when the
`git` spawn returns an error. Sampling it twice can return two different
answers, and a barrier that fires on the disagreement between two samples would
be exactly the kind of false positive this ADR is trying not to introduce.

**A plugin that has nothing to do here is not a failure.** `applicable` counts
only the plugins that said yes on this pass. Every provider gates
`is_applicable` on config that is empty by default — sqlite on `db_path`, mysql
and postgres and supabase on `database`, docker on the `aegis.snapshot` label
(ADR-039 item 3). A configured-but-inapplicable provider never enters the count
and can never produce a degradation. This is the trade-off the issue asked to
have written down, and it is why the count is taken inside the snapshot loop
rather than from the registry's configured provider list.

**The two reasons stay separate.** `RecoveryDegradation` gains
`PartialSnapshotCoverage` next to `NoSnapshotAvailable`; the enum is
`#[non_exhaustive]`, so this is additive. The audit log records
`partial_snapshot_coverage`, and a reader can tell "nothing was captured" from
"some of it was" without parsing a shared reason string. The Recovery override
prompt reads from the same reason: saying "No required Snapshot was created"
over a partial attempt would be a false statement, and a human who believes it
will misjudge what running anyway costs.

Total failure is unchanged. An empty record set is `NoSnapshotAvailable`
whether zero plugins applied or five applied and all five failed, and the
non-interactive path still fails closed the same way.

## What this changes in ADR-031

ADR-031 §8 keeps `RecoveryStatus` as "`Ready` when at least one attempt is
`Ready`", and §9 states that a partial failure lives only in a per-attempt
array, never in `recovery_degradation`. Neither the per-attempt readiness levels
nor the `snapshot_attempts` array is implemented yet, so nothing in the code
depends on that rule today. This ADR replaces it along one axis: an applicable
plugin that produced no record is a degradation of the obligation, not just a
row in an array.

The rest of ADR-031 stands. When the per-attempt array and the
`SnapshotArtifactUnavailable` / `SnapshotArtifactInvalid` reasons land, they
join `PartialSnapshotCoverage` rather than replacing it, and the fixed priority
ADR-031 §9 defines for picking one main reason extends to cover it: an artifact
that exists and is provably corrupt still outranks a plugin that produced
nothing. The one line that does not survive is "`recovery_degradation` is never
set while the obligation is met", because under this ADR a pass with a failed
applicable plugin does not meet the obligation.

## Consequences

- An effect-opaque command in a repository where git snapshots succeed and a
  configured database provider fails now prompts instead of running silently.
  The human can still choose Run once, and that choice is recorded with the
  reason that produced it.
- `SnapshotCoverage` is a new public type in `aegis-snapshot`, re-exported
  through `aegis::snapshot`. `snapshot_all`, `RuntimeContext::create_snapshots`
  and `create_snapshots_async` return it instead of `Vec<SnapshotRecord>`.
- `show_recovery_override_with_input`, `show_recovery_override_decision` and
  `show_recovery_override_via_tty` take the `RecoveryDegradation` being decided.
- The confirm-then-snapshot ordering is untouched. Snapshots are still created
  only after a Danger command is approved (`src/shell_flow.rs`), because
  creating one for a command the human is about to deny would be a side effect
  before consent. This ADR changes what happens after the snapshot pass, not
  when it runs.
- `CONTEXT.md`'s `Recovery degradation` and `Recovery status` entries are
  updated, and `Snapshot coverage` is added as a term.
- The confirmation dialog itself still shows applicable plugins from the
  planning-stage sample, which runs before any attempt. Surfacing a *predicted*
  partial failure there is not possible and is not attempted: the failure is
  only knowable after the attempt, which is what the Recovery prompt is for.
