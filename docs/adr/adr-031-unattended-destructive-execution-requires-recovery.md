# ADR-031 — Unattended destructive execution requires Recovery

## Status

Accepted. Extends [ADR-016](adr-016-effect-opaque-execution-uses-recovery-backstops.md)
and partially supersedes [ADR-004](adr-004-snapshots-are-best-effort-audit-is-append-only.md).

## Context

[ADR-030](adr-030-the-confinement-profile-is-derived-from-the-assessment.md) §10
split one question out of confinement: whether Aegis must hold a verified
`Recovery` before it executes an approved destructive command. Confinement
answers what authority a command gets; recovery answers whether an
already-approved destructive effect can be undone. Both feed the same product
claim — that Aegis limits the damage after a mistaken Allow — and both must be
revisable independently.

Rebaselining 1.0 scope
([#232](https://github.com/IliasAlmerekov/aegis-shellguard/issues/232)) asked the
question. Four facts constrained the answer, and one premise of the ticket died
on contact with the code.

**`Required recovery` does not cover `Danger` today.** The ticket asserted it
did. `recovery_status` returns `None` whenever `!effect_opaque`
(`src/runtime/recovery.rs:22`), so the obligation and its degraded/override
handling are reachable only for `Effect-opaque execution`. For ordinary `Danger`
the snapshot is best-effort per ADR-004, and `snapshots_required` is not even
requested when no plugin is applicable
(`crates/aegis-policy/src/engine.rs:279`). `recovery_backstop_applies`
(`src/planning/core.rs:70`) names `Danger` only to resolve the applicable plugin
set, not to require anything of it.

**Exactly three paths reach `Danger` without a dialog**: `AllowlistOverride`,
which needs `allowlist_override_level = "Danger"` and is not the default
(`engine.rs:159`); `PolicyRulesOverride` with `PolicyRuleDecision::Allow`; and
`Mode::Audit`, which declines all enforcement by design (`engine.rs:71`).
`PromptDecision::ApproveAlways` writes a `[[allow]]` entry
(`src/shell_flow.rs:82` → `crates/aegis-config/src/amend.rs:114`), so under the
default `allowlist_override_level = "Warn"` its rule does **not** auto-approve
the `Danger` command it was created for. The hazard population is therefore
exactly those operators who deliberately raised the ceiling or wrote an explicit
`Allow` rule — which strengthens the case for a per-run gate rather than
weakening it, because the cost falls only on configurations that opted out of the
dialog.

**No verification runs on the execution path.** ADR-026 §3 introduced
`aegis snapshot verify` as a separate, deliberately uncached command, and
`rollback` performs its own preflight. Nothing checks the snapshot Aegis just
created before the command that needs it runs.

**`ApproveAlways` persists before the snapshot is attempted.** The rule is
appended at `src/shell_flow.rs:82`; `execute_with_snapshots` does not run until
line 160. A persisted approval can therefore outlive the recovery that justified
it, and it never knew about it in the first place.

## Decision

Decided in [#232](https://github.com/IliasAlmerekov/aegis-shellguard/issues/232).

**1. The obligation attaches to the absence of a human decision, not to
`RiskLevel`.** `Required recovery` applies to `Unattended destructive execution`:

```
Mode != Audit
&& snapshot_policy != None
&& no human decision for this run
&& (Assessment is Effect-opaque || RiskLevel == Danger)
```

`Effect-opaque execution` is one case of it — it stays `Safe` and never earns a
dialog. Auto-approved `Danger` is the second. Interactive `Danger` is **not**
covered: a human decides now, and ADR-026 §4 already gives that human a red
disclosure plus a full-word `yes` when no provider applies. `Unattended` is
defined by the missing decision, never by a missing TTY: a TTY can be present
while the command is auto-approved, and a TTY can be absent under a previously
persisted `ApproveAlways`.

**2. `Required recovery` is not extended to every `Danger`.** Making it so would
refuse a recursive delete of `node_modules` outside a git repository — ordinary
agent work already covered by informed consent. The honest 1.0 promise for
interactive `Danger` is therefore weaker than "Aegis always protects you after a
mistaken Allow", and must be written that way: Aegis creates a `Snapshot` when a
provider applies, discloses the absence of `Recovery` explicitly, and bounds the
blast radius through the Sandbox. A mistakenly confirmed deletion with no
applicable provider can still be irreversible.

**3. `snapshot_policy = None` survives as a trusted opt-out, but stops being
silent.** ADR-029 removed `sandbox.required` on the grounds that a mandatory
layer cannot also be skippable. Recovery is not symmetric with the Sandbox: the
Sandbox is always applicable, while a snapshot provider can be physically absent
(no git repository, no Docker). Removing the key would make every interpreter
invocation outside a repository prompt. It stays, and the project security
ratchet already forbids a project config from setting it
(`crates/aegis-config/src/model/ratchet.rs:434`). What changes is visibility:
every affected Audit entry carries an explicit `Recovery opt-out` marker, and the
posture is reported once per session. `Mode::Audit` is the second trusted opt-out
— broader, since it declines prompts and blocks too. Neither is a
`Recovery degradation`, and neither may be recorded as one.

**4. A persisted approval is not a persisted permission to run without
`Recovery`.** Two separate gates:

- *Persistence gate.* `ApproveAlways` persists its rule only after
  `Recovery status = Ready`. If readiness fails, no rule is written, the human is
  told the rule was not saved, and the only remaining choices are a one-time
  override or refusal. Under the default `allowlist_override_level` this changes
  nothing observable for `Danger`, because the rule was inert anyway — the
  substance is the next gate.
- *Per-run gate.* Every subsequent auto-approved `Danger` re-creates the
  `Snapshot` and re-checks readiness. `Ready` executes with no confirmation;
  otherwise execution halts and offers only a one-time `Recovery override`.
  Without this, one successful snapshot would license every later run, including
  every later provider failure.

While `snapshot_policy = None` is set, a destructive command can still be
approved once, but no persistent approval for it can be created:
"Persistent approval is unavailable while Snapshot recovery is disabled." This is
consistent, because `None` declines `Recovery` — not every other Aegis limit.

**5. An unsatisfiable obligation halts; it does not evaporate.** When an
auto-approved `Danger` has no applicable provider, the persisted rule is not
deleted, but it does not authorise this run: Aegis asks for a one-time human
decision, and automatic execution resumes once a provider is available again.
Letting "no provider" pass would make provider absence the quietest way around
the gate — the exact asymmetry ADR-016 already rejected, where absence of a
plugin is evidence that recovery is unavailable, not that it was never required.
Without a TTY this fails closed, and the incompatibility is stated plainly: **a
persisted `Allow` rule for a `Danger` command does not guarantee execution in CI;
with no `Ready` Recovery the command is refused even though the rule matched.**
`ci_policy` remains a separate axis and does not waive the Recovery gate.

**6. Verification is a local artifact check, not liveness.** The mandatory
contract per attempt is: the expected artifact exists, is readable, and is not
empty. A structural check — reading a dump's table of contents with a local
tool — is best effort on top of it:

| Outcome | `readiness` | `validation_level` | `structural_check` |
| --- | --- | --- | --- |
| checking tool absent | `Ready` | `PresenceOnly` | `ToolUnavailable` |
| tool exceeded the budget | `Ready` | `PresenceOnly` | `TimedOut` |
| no check exists for the format | `Ready` | `PresenceOnly` | `NotSupported` |
| tool reported corruption | `Invalid` | `PresenceOnly` | `Passed` |
| check succeeded | `Ready` | `Structural` | `Passed` |

A missing checking tool is a fact about the machine, not about the artifact;
treating it as corruption would block work over an unfound binary. This is
deliberately **not** the ADR-026 §3 liveness check, which stays a separate
uncached command: a local read-back does not prove the dump is complete, the
database reachable, or the `Rollback` able to finish. Budget: **≤ 100 ms per
attempt, ≤ 500 ms per command, local filesystem only, no network**. The `< 2 ms`
safe-hot-path budget does not apply — this work runs only after a `Snapshot` has
been created on the execution path, which already costs orders of magnitude
more.

**7. The halted unattended path uses the `Recovery override` dialog, not the
`Danger` confirmation.** The two ask different questions: the ordinary
confirmation asks whether a dangerous operation is permitted; the override asks
whether execution may proceed with no required recovery path. An auto-approved
`Danger` has no ordinary dialog, so only the override appears. It adopts
ADR-026 §4's full-word rule — `Type "yes" to run once without Recovery` — because
the consequence is identical to the case that rule was written for: irreversible
execution with no way back, too cheap to authorise with a single `y`. The
override is never persisted, fails closed without a TTY, and records its specific
`RecoveryDegradation`. When a human has just chosen `ApproveAlways` and readiness
then fails, the second dialog is not a duplicate: the first authorised the
command and requested persistence, the second discloses a new runtime
degradation.

**8. Three levels of state, kept separate.**

- **Per attempt** — `Snapshot attempt readiness`: `Ready` (passed the local level
  reached), `Invalid` (artifact found, failed a minimal check), `Unavailable` (an
  identifier exists but the artifact is missing or unreadable, or the attempt
  produced no `SnapshotRecord` at all).
- **Per obligation** — `RecoveryStatus`: `Ready` when at least one attempt is
  `Ready`, else `Degraded`. Unchanged.
- **Per degradation reason** — `RecoveryDegradation` gains
  `SnapshotArtifactUnavailable` and `SnapshotArtifactInvalid` beside
  `NoSnapshotAvailable`, so the reason stays queryable; the enum is
  `#[non_exhaustive]` for exactly this. There is no `NotRequested` variant: an
  absent obligation is not a degradation.

**9. `recovery_degradation` is never set while the obligation is met.**
`recovery_status: Ready` with `recovery_degradation: SnapshotArtifactInvalid`
would be a self-contradictory line. A partial failure lives only in the new
per-attempt array:

```
recovery_status: Ready          recovery_status: Degraded
recovery_degradation: null      recovery_degradation: SnapshotArtifactInvalid
snapshot_attempts:              snapshot_attempts:
  - git: Ready                    - git: Unavailable
  - postgres: Invalid             - postgres: Invalid
```

When nothing is `Ready`, the single main field takes one value by fixed priority
— `SnapshotArtifactInvalid` > `SnapshotArtifactUnavailable` >
`NoSnapshotAvailable` — because an artifact that exists and is provably corrupt
is the most alarming and the least likely to be noticed. The main field answers
for the obligation and keeps queries compatible; the array carries the full
provider-specific picture.

**10. `snapshot_attempts` records attempts, not artifacts.** One element per
attempt actually made, carrying `plugin`, `readiness`, `validation_level`, and
`structural_check`. An inapplicable provider is absent — inapplicability is a
fact about the directory, already conveyed by `NoSnapshotAvailable` and the
applicable-plugin set, and a placeholder row would blur "never tried" into
"tried and failed". An attempt that errored without a `SnapshotRecord` appears as
`Unavailable`. The array is written for best-effort snapshots too, not only
required ones: it is a factual record of attempts, not a projection of the
obligation. It is named for the attempt rather than the artifact because an
element can exist when no artifact does.

**11. The new fields enter the integrity chain immediately.** They go into
`AuditIntegrityPayload` under the ADR-022 §10 precedent: with
`skip_serializing_if = "Option::is_none"`, no historical line serializes them,
the legacy payload stays byte-identical, and old chains keep verifying — while a
tampered readiness result on a new line breaks the chain. The "hash format
predates the field" argument that excluded `sandbox_status`
(`crates/aegis-audit/src/logger/integrity.rs:52`) applies only to fields already
being written. Fixtures are mandatory: a legacy line still verifies, a new line
verifies, altering one provider's readiness breaks verification, and reordering
or deleting an array element is detected.

That `effect_opaque`, `snapshots_required`, and `recovery_degradation` are today
**outside** the payload is a separate defect — the record that a command ran
unprotected is not covered by the chain. It cannot be fixed here: those fields
are `Some` on every post-ADR-016 entry, so adding them would break verification
of all of them. Closing it needs the `chain_alg` versioning that
`integrity.rs:60` already describes, and it does not block this ADR.

**12. The session posture is reported to both audiences.** `additionalContext`
reaches the model; it is not a human-visible warning. A `SessionStart` hook
therefore returns both fields — `systemMessage` for the person,
`additionalContext` for the agent — and neither substitutes for the other. On the
Shell transport the human reads stderr; on hook transports the human's guaranteed
surface is the confirmation dialog, where the `Recovery opt-out` line sits beside
ADR-026 §4's red disclosure rather than replacing it.

The hook scripts (`scripts/hooks/claude-session-start.sh`,
`scripts/hooks/codex-session-start.sh`) read only environment variables and the
toggle file today, so the effective posture must come from the binary. The core
CLI stays transport-neutral — `aegis session-status --format json` returning
typed state such as `{"enforcement": "enabled", "recovery":
"disabled_by_trusted_config"}` — and each agent adapter builds its own envelope.
If the binary is missing, fails, or returns anything unparsable, the hook still
emits exactly one valid JSON document reporting that the status could not be
determined, to both audiences, and never leaks stray stdout or stderr into the
protocol.

**This is not a conflict with ADR-007.** `SessionStart` is an informational
surface and makes no execution decision. An indeterminate status here is
fail-loud *informing*, not fail-open *enforcing*; `PreToolUse` independently
continues to refuse when Aegis cannot be invoked (ADR-023).

## Consequences

- **ADR-004's snapshot half is partially superseded.** Its blanket "snapshots are
  provider-based and best-effort" becomes: best-effort while a human decides for
  the current run; required otherwise. Its Audit decisions — append-only, no
  external anchor — remain in force, so a wholesale supersession would misstate
  what changed. ADR-016 stays `Accepted`; its sentence that ordinary
  non-effect-opaque `Danger` failures retain ADR-004's contract is now false for
  the unattended subset and is annotated in place.
- **`CONTEXT.md` gains four terms** — `Unattended destructive execution`,
  `Snapshot attempt readiness`, `Validation level`, `Recovery opt-out` — and
  `Required recovery` is rewritten around the invariant rather than around
  "before a command". `Recovery status` is unchanged: it is already the derived
  obligation-level fact, and now simply has a second input.
- **A CI pipeline can break by design.** An `Allow` rule for a `Danger` command
  in a directory with no applicable provider stops executing unattended. This is
  the accepted price of fail-closed, not an oversight.
- **Watch needs no new transport work.** It already prompts over `/dev/tty`
  (`src/watch/runner.rs:274`) rather than the protocol stdio, and derives its
  behaviour from the same shared `recovery_status` fact, so extending the
  predicate reaches both surfaces.
- **The gate lands in a file already over budget.** `src/shell_flow.rs` is 815
  lines against an 800-line limit
  ([#228](https://github.com/IliasAlmerekov/aegis-shellguard/issues/228)), so the
  refactor precedes the first line of this work.
- **Observation ships before behaviour.** The Audit schema and its integrity
  fixtures land first, then the runtime gate fills them. The reverse order would
  merge a release in which behaviour has changed but the Audit log cannot yet
  explain why a command was allowed or refused.
- **ADR-026 §4 is still unimplemented.** No "cannot be rolled back" disclosure
  exists in the tree, so the interactive-`Danger` half of the honest promise is
  owed by
  [#215](https://github.com/IliasAlmerekov/aegis-shellguard/issues/215), not by
  this ADR.
