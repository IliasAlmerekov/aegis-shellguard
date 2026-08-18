# ADR-027 — One 1.0 release gate, and it lives in the issue tracker

## Status

Accepted

## Context

`TASKS.md` was created as the normalized backlog index for two sources: the
2026-06-23 reviewer security audit and the 2026-06-24 live crash-test of
`aegis 0.5.9`. Each finding carries an id (`C1`…`C4`, `H1`…`H9`, `M1`…`M10`,
`P3-1`…`P3-9`), a finding statement, acceptance criteria, a status, and
traceability. `AGENTS.md` made the file part of the session ritual: read it to
learn the open blockers (step 4 of onboarding), and flip `[ ]` to `[x]` when a
finding closes (step 5 of the change checklist).

Two problems surfaced while rebaselining 1.0 scope
([#199](https://github.com/IliasAlmerekov/aegis-shellguard/issues/199)).

**The gate had three divergent copies.** The set of findings blocking 1.0 was
written down in `TASKS.md`, again in `PROJECT_STATE.md` (the blocker table at
lines 2248–2335), and again in `docs/release-readiness.md`. Prose copies do not
update themselves: `TASKS.md`'s "Current implementation order" still presented
H6, H7a/b, H9, M3a, M4, and M1 as upcoming when all six were closed, and
`PROJECT_STATE.md` still announced "M4 → M7 is next". Nothing forced either list
to change when a finding closed, so both drifted silently — and
`PROJECT_STATE.md` is the file agents are told to read first, so the stale copy
reached context before the accurate one.

**Severity was standing in for an argument.** The file's implicit gate was the
severity prefix: P0–P2 block 1.0, P3 does not "unless an implementation review
promotes them". That made "does M2 block 1.0?" a question about a letter rather
than about the product promise, and it left `P3-4` — a `_ => PromptDecision::Approve`
wildcard that auto-approves any future `RiskLevel` variant, the only fail-open
default in the code — sitting below the release line on grounds of severity
label alone.

Meanwhile the work itself had already moved to the issue tracker: `M5` was
decomposed into [#188](https://github.com/IliasAlmerekov/aegis-shellguard/issues/188)
and its children, ADR-024 and ADR-026 spawned
[#212](https://github.com/IliasAlmerekov/aegis-shellguard/issues/212)–[#215](https://github.com/IliasAlmerekov/aegis-shellguard/issues/215),
and a containment defect found during the rebaseline
([#211](https://github.com/IliasAlmerekov/aegis-shellguard/issues/211)) existed
only as an issue with no `TASKS.md` id at all. The document and the tracker were
describing the same release from two places.

## Decision

Decided in
[#202](https://github.com/IliasAlmerekov/aegis-shellguard/issues/202).

**1. The gate criterion is the promise, not the severity.** A finding blocks 1.0
if and only if it falsifies something `PRD.md` promises for 1.0 — fail-closed
classification, containment, Snapshot/Rollback, audit integrity, or the stated
product contract. The severity prefix stays as a historical label and carries no
release meaning. A `P3` that is fail-open blocks; a `P2` whose acceptance
criteria only sharpen a documented non-goal does not.

**2. Open work lives in issues, and the gate is milestone membership.** Every
open task is an issue. Membership in the `1.0` milestone *is* the statement "this
blocks 1.0", so release readiness becomes a checkable fact — the `1.0` milestone
has no open issues — instead of a checklist to be reconciled across documents.
No parallel `1.0-blocker` label: a second expression of the same set would drift
from it.

**3. `TASKS.md` becomes a historical registry of findings.** It keeps the
`id → finding → issue` mapping and drops statuses, acceptance criteria, and the
"Current implementation order" section. It carries no "blocks 1.0" column,
because that column is milestone membership. `PROJECT_STATE.md` likewise
references the milestone instead of restating the blocker table. The id namespace
survives because ADRs, plans, and tests already point into it
(`tests/audit_integrity_wording.rs`, `tests/agent_hooks_install.rs`,
`tests/toggle_parity.rs`, `crates/aegis-scanner/src/scanner/tests/edge_cases.rs`,
ADR-013/014/015/026, and seven files under `docs/plans/`).

**4. Implementation order is expressed as issue dependencies.** The prose order
is deleted rather than relocated. Ordering lives in native blocked-by
relationships between issues inside the milestone, which update themselves when a
blocker closes.

**5. New findings are inducted as issues.** A defect found after the two original
audits — `#211` is the first — enters the milestone directly and gets a
`TASKS.md` id only if some other artifact needs to reference it by id. There is
no second reservoir of blockers outside the milestone.

The verdicts reached under criterion 1 are recorded on
[#202](https://github.com/IliasAlmerekov/aegis-shellguard/issues/202); this ADR
records the mechanism, not the individual calls.

## Consequences

- Release readiness is machine-checkable and singular. "Is Aegis 1.0 ready" has
  exactly one answer source, and no document can contradict it while staying
  internally consistent, because no document holds a copy.
- `AGENTS.md` changes: onboarding step 4 points at the `1.0` milestone rather
  than at `TASKS.md`, and the change-checklist step "flip `[ ]` to `[x]`" is
  replaced by closing the issue. Agents that learned the old ritual will look for
  checkboxes that are no longer there — the rewritten `TASKS.md` header says
  where the gate went.
- The self-contained offline checklist is lost. Reading the repository alone no
  longer tells you what blocks 1.0; that now requires the tracker. This is the
  trade accepted in exchange for a single source, and it is why the `id → finding`
  mapping stays in-tree: the historical references must keep resolving without
  network access.
- Traceability gains a hop. `ADR-013`'s "see TASKS.md H5" now resolves to a
  finding statement plus an issue link rather than to a status. The finding text
  is what those references were always citing.
- Two allowlist entries in `tests/audit_integrity_wording.rs` point at `TASKS.md`
  lines that the rewrite removes. The guard only flags occurrences of banned
  phrasing and never asserts that an allowlisted line still exists, so the
  entries become dead rather than failing — they can be pruned when convenient.
- Findings that are neither closed nor 1.0 blockers need somewhere to live.
  They stay open issues outside the milestone; absence from the milestone is the
  only marker of "1.x", so an issue left milestone-less by accident silently
  reads as deferred.

## Alternatives considered

**Keep `TASKS.md` as the gate and fix the copies.** Rejected: the copies were
already fixed once and drifted again. The failure is structural — three prose
lists with no mechanism forcing them to agree — and fixing instances does not
remove it.

**Delete `TASKS.md` entirely.** Rejected: `C1`…`P3-9` is a vocabulary that ADRs,
plans, and test comments already speak. Deleting the file would leave those
references pointing at nothing.

**Milestone plus a `1.0-blocker` label.** Rejected for the same reason as the
divergent copies: two expressions of one set, and nothing keeping them equal.

## References

- [`TASKS.md`](../../TASKS.md)
- [`AGENTS.md`](../../AGENTS.md)
- [`PROJECT_STATE.md`](../../PROJECT_STATE.md)
- [ADR-024](adr-024-language-aware-analysis-ships-opt-in-and-is-not-a-1-0-release-gate.md)
- [ADR-026](adr-026-snapshot-rollback-contract-for-1-0.md)
- [#199 — Map: rebaseline Aegis 1.0 scope](https://github.com/IliasAlmerekov/aegis-shellguard/issues/199)
- [#202 — Which open findings block 1.0](https://github.com/IliasAlmerekov/aegis-shellguard/issues/202)
