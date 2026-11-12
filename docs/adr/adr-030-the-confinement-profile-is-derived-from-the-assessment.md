# ADR-030 — The confinement profile is derived from the Assessment

## Status

Accepted. Extends [ADR-029](adr-029-the-sandbox-is-a-mandatory-1-0-layer.md).
Refined 2026-08-20 — see [Refinement](#refinement--2026-08-20-the-ceiling-under-the-sandbox-migration-contract).
Amended 2026-10-06: 1.0 ships the `Trusted ceiling` only, and per-command
derivation moves to 1.x. See
[Amendment](#amendment--2026-10-06-10-ships-the-trusted-ceiling-only).

## Context

[ADR-029](adr-029-the-sandbox-is-a-mandatory-1-0-layer.md) made the Sandbox a
mandatory 1.0 layer, and justified the second confinement layer in these words:
"Aegis confines each command according to what its own rule corpus knows that
command destroys." That sentence describes a **per-command** profile. No such
code exists.

Today `SandboxConfig` carries three fields — `allow_write`, `allow_network`,
`required` — and is built once per invocation from `AegisConfig`
(`src/runtime/context.rs:55`), before any classification has happened. Every
executed command receives the same profile, which is exactly the shape Codex
ships (`workspace-write` for the whole session). Under that design the second
layer adds little beyond the first.

Rebaselining 1.0 scope
([#209](https://github.com/IliasAlmerekov/aegis-shellguard/issues/209)) asked
whether the profile should instead be computed from the command's `Assessment`,
and whether that is 1.0 work. Three facts constrained the answer.

**The Sandbox already applies to Safe commands.** `src/shell_flow.rs:76` passes
the same `sandbox_config` down the `ExecutionDisposition::Execute` branch, which
is the `AutoApproved` path. Profile construction is therefore inside the
`< 2 ms` p99 safe-hot-path budget (`PRD.md:209`), not after it as
[#209](https://github.com/IliasAlmerekov/aegis-shellguard/issues/209) assumed.

**The rule corpus is a corpus of dangers, not of needs.** A rule fires and says
what a command destroys. `cargo build` matches nothing: its `Assessment` is
`risk: Safe`, `matched: []`. Deriving "cargo build needs its build directories
and `/tmp`" is not a projection of that corpus — it requires a second,
open-ended corpus of what every tool legitimately needs, whose every gap is a
hard command failure with no dialog in front of it.

**A rule carries no resource information.** `Pattern` holds
`id / category / risk / pattern / description / safe_alt / justification /
source`. Target extraction from argv exists only in
`crates/aegis-scanner/src/scanner/recursive.rs`, and only for the recursive case
(ADR-025).

## Decision

Decided in [#209](https://github.com/IliasAlmerekov/aegis-shellguard/issues/209).

**1. Derivation is 1.0 work.** ADR-029 already promises command-aware
confinement; 1.0 delivers it rather than shipping a justification for an absent
capability.

**2. The static profile is retained as the `Trusted ceiling`.** It stops being
the command's final profile and becomes the upper bound on permissions. Its
default is a working one, because a mandatory layer whose default forbids all
writes is unusable: write access to the workspace tree and to `/tmp`, network
off. A `cwd` of `/` does not implicitly make `/` a workspace.

**3. Derivation only ever subtracts, and only where a rule fired.**

```
effective confinement profile
    = trusted ceiling
    ∩ project tightening
    ∩ restrictions from matched rules
    ∩ outer agent sandbox
```

An empty `matched` yields the `Trusted ceiling` unchanged. Several matched rules
intersect. A rule — including a user rule from `.aegis.toml` — can never add a
path, a network permission, or any other authority the ceiling withholds. This
matches the ADR-013 ratchet, and matches what
[#208](https://github.com/IliasAlmerekov/aegis-shellguard/issues/208) measured at
the kernel level: a nested layer can only tighten.

The consequence is stated plainly rather than hidden: derivation does not try to
learn what a command needs. It starts from a working ceiling and removes only
what a matched rule can justifiably remove. Commands Aegis does not recognize run
under the full ceiling.

Because an unmatched command derives nothing, the safe hot path performs no
derivation work and the `PRD.md:209` budget is untouched.

**4. `Confinement restriction` is an optional typed field on the Rule itself**,
shared by every rule kind (regex `Pattern` and prefix rule alike), so no part of
the corpus sits outside the model. `Category` takes no part in derivation:
`Filesystem` alone spans `rm`, `chmod`, `dd`, `mv`, and `truncate`, whose operands
sit in different argv positions, and a category-derived default would silently
produce a wrong profile — which here means a broken command with no dialog.

Targets are never located by argv index. A restriction names one of a small
closed set of program-specific extractors that know their command family's
syntax, may return several targets, and admit no arbitrary expressions and no
shell fragments.

`confinement: None` is the identity: the `Trusted ceiling` passes through
unchanged.

**5. An unresolved extractor degrades, it does not block.** When a restriction is
declared but its extractor resolves no target, the command runs under the
`Trusted ceiling` and a `Confinement degradation` is recorded. Blocking was
rejected: such a command runs today, so blocking would turn a parser gap into an
outage and make Aegis worse than it was before the feature.

The degradation must be **visible in the confirmation dialog**, not only in the
Audit log. This mirrors `Recovery degradation`, of which `CONTEXT.md` says it
"must never silently become permission to execute". A human approving a forced
recursive delete has to see that the blast radius is the whole workspace rather
than the directory the rule named.

**6. The 1.0 coverage gate is the `FS`, `GIT`, and `DB` families.** Every rule at
`RiskLevel::Danger` or `Block` in those three carries either a
`ConfinementRestriction` or a recorded reason for `None`. They are the families
where narrowing writes and network is meaningful, and where the product claim
lives. `PS`, `PKG`, `DK`, and `CL` are excluded deliberately: the Sandbox governs
writes and network, so there is nothing to narrow for a signal sent to one PID,
and requiring an annotation there would be theatre. Documentation states the
actual coverage; no document may claim that every matched rule narrows the
Sandbox while any rule carries `None`.

**7. The effective profile is recorded in the Audit log** as a new optional
field: write roots, network, the profile source (`baseline` / `derived` /
`degraded`), and the IDs of the restricting rules. ADR-021 requires reporting the
actual execution path. While the profile was session-wide, `sandbox_status:
active` was a complete answer because the profile sat in the config; once the
profile varies per command, `active` no longer says *what* was active.

**8. `confinement_required` is not reused.** The field exists already, hardwired
`false`, reserved by ADR-016 as "the optional stricter tier"
(`src/runtime/context.rs:505`). Its sense is "was confinement required", which
ADR-029 settled permanently as `true` on Linux and macOS. The derived profile
goes into a new field. Reusing the old one would give a single name two senses —
what `CONVENTION.md` §11 forbids.

**9. The confirmation dialog shows the profile on every prompt**, as one compact
line: where the command may write, and whether it has network. The human's Allow
is a decision about blast radius. Per ADR-029 §7 the line names writes and
network only and must never suggest that file reads or secrets are hidden; the
six confidentiality overclaims pinned by `tests/contracts_docs.rs` stay banned.

**10. Recovery after approval is out of this ADR.** Whether Aegis must hold a
verified `Recovery` before executing an approved destructive command is a
separate decision with its own trade-offs — it removes an existing opt-out
(`snapshot_policy = None` disables the backstop today,
`src/planning/core.rs:75`), adds latency to the execution path, and changes the
meaning of a persisted approval. It is owned by
[#232](https://github.com/IliasAlmerekov/aegis-shellguard/issues/232) and must be
revisable independently of derivation.

## Consequences

- **`CONTEXT.md` gains four terms** — `Trusted ceiling`, `Confinement
  restriction`, `Effective confinement profile`, `Confinement degradation` — and
  the bare phrase "confinement profile" stops being a term. The `Sandbox` entry
  currently defines Sandbox *as* "an OS-level confinement profile optionally
  applied", which now collides twice: with the new vocabulary, and with ADR-029's
  removal of "optionally".
- **The default `[sandbox]` profile changes.** `docs/config-schema.md:270`
  documents `allow_write = []`, which under a mandatory layer forbids a build
  from writing `target/` and a commit from writing `.git/`. The documented
  default and `SandboxSettings::default()` both move to the workspace tree plus
  `/tmp`.
- **The Audit schema grows one optional field**, written with the existing
  `#[serde(default, skip_serializing_if = "Option::is_none")]` convention so
  legacy lines stay byte-identical and their integrity hash is unchanged.
- **Every extractor needs positive and narrowness tests** — that it finds the
  targets it should, and that the resulting profile grants nothing beyond them.
- **A wrong restriction is a hard failure, not a prompt.** This is the feature's
  central risk and the reason `Category` inference and argv indices are both
  refused.

## Refinement — 2026-08-20: the ceiling under the `[sandbox]` migration contract

Decided in [#240](https://github.com/IliasAlmerekov/aegis-shellguard/issues/240)
and recorded as an amendment to
[ADR-029](adr-029-the-sandbox-is-a-mandatory-1-0-layer.md). Decision 2 above gave
the `Trusted ceiling` a working default; that decision alone did not say what an
existing `[sandbox]` section does to it. Four points refine it.

**1. The computed default applies only to an absent field.** `allow_write` absent
yields the computed default of decision 2 — workspace tree plus `/tmp`. A present
`allow_write` is an explicit **override** of that default, never an addition to
it. "The full writable set" is unexpressible, because the workspace is resolved at
the runtime boundary rather than being a literal a user can type; treating a list
as an addition would silently widen authority for a config that believed it stated
everything.

**2. An explicit `[]` is valid, and an empty effective ceiling gets no fallback.**
Decision 2 says an empty *default* is unusable for ordinary work; it does not make
an explicit empty ceiling invalid. `allow_write = []` is accepted and means zero
configured writable roots. A ceiling emptied by an explicit `[]` **or** by omitted
entries stays empty: falling back to the computed default would be fail-open, and
the one place Aegis grants authority the config never asked for — precisely at the
moment it failed to understand the config. The cost is stated rather than hidden:
an upgrade can leave an installation with no configured writable roots, where
commands fail on write rather than with a clear message.

**3. `Trusted ceiling path omission` is not a `Confinement degradation`.**
Decision 5 gives `Confinement degradation` a narrow meaning: a rule declared a
restriction, its extractor found no target, and the command received the whole
`Trusted ceiling` — the profile **widens**. A malformed ceiling entry is the
opposite movement: the entry is excluded and the profile **narrows**. It gets its
own typed outcome, `trusted_ceiling_path_omitted`, with a reason
(`relative` / `parent_dir` / `not_found` / `outside_trusted_ceiling`), a layer, and
a location. Reusing `Confinement degradation` would blur this ADR, `CONTEXT.md`,
and the metric that counts degradations.

**4. The Audit log stores the resulting profile, not the omission reasons.**
Decision 7's new field keeps its shape: the effective profile actually granted.
The causal diagnostics of how that profile was built belong to config and runtime
diagnostics — the Audit log is not a history of config provenance. Both arguments
that would have forced the reasons out of the record are false and were not used:
a new optional field enters `AuditIntegrityPayload` from day one with
`skip_serializing_if`, and an omission is not always static, since `not_found` and
a canonical symlink escape depend on filesystem state.

The per-field matrix and the containment rules are in the resolution of
[#240](https://github.com/IliasAlmerekov/aegis-shellguard/issues/240); the
normative statement is `PRD.md` §5.5. Containment is checked at two moments —
component-wise at merge with no filesystem access, and canonically at enforcement
on both the trusted and the effective roots — which requires the trusted base and
its provenance to reach enforcement
([#242](https://github.com/IliasAlmerekov/aegis-shellguard/issues/242)).

## Amendment — 2026-10-06: 1.0 ships the Trusted ceiling only

Decided in the 2026-10-06 positioning grill and recorded in
[#498](https://github.com/IliasAlmerekov/aegis-shellguard/issues/498). The
Decision above is unchanged as the target design. Only its release split changes.

**1.0 ships the `Trusted ceiling` and nothing narrower.** The 1.0 effective
profile is the ceiling of decision 2 intersected with project tightening and with
any outer agent sandbox
([#242](https://github.com/IliasAlmerekov/aegis-shellguard/issues/242)). Every
command executed outside `Mode::Audit` gets that profile. The Sandbox stays mandatory under
[ADR-029](adr-029-the-sandbox-is-a-mandatory-1-0-layer.md), and the ceiling rules
of the 2026-08-20 Refinement stay normative for 1.0.

**Derivation and surfacing move to 1.x.** Intersecting the restrictions of matched
rules ([#243](https://github.com/IliasAlmerekov/aegis-shellguard/issues/243)) and
showing the effective profile and its `Confinement degradation` in the dialog and
the Audit log ([#244](https://github.com/IliasAlmerekov/aegis-shellguard/issues/244))
are 1.x target state. Until then a rule carries no `Confinement restriction`, no
`Confinement degradation` can occur, and the confinement degradation metric is not
a 1.0 metric.

**Why.** Rollback became the main promise of Aegis. Agent platforms now ask before
a destructive command, but none of them restores the effects of a shell command.
1.0 effort goes to the Snapshot and Rollback contract of
[ADR-026](adr-026-snapshot-rollback-contract-for-1-0.md). Per-command narrowing
needs a program-specific extractor for every restriction, and a wrong one is a
hard command failure with no dialog in front of it (Consequences above). It is
not needed to keep the 1.0 promise.

## Refinement — 2026-11-12: what the workspace is

Decided in the triage of
[#242](https://github.com/IliasAlmerekov/aegis-shellguard/issues/242). Decision 2
gave the default `Trusted ceiling` "the workspace tree plus `/tmp`" and said only
that a `cwd` of `/` is not a workspace. The agent controls its working directory
across calls, so a ceiling root taken from it as-is lets the agent choose its own
write authority with `cd`. Seven points close that.

**1. The `Workspace` is the root of the enclosing git worktree.** Aegis walks up
from the working directory to the first `.git` entry, file or directory, using
`stat` only. It never spawns `git`: confinement runs on every command, and a
subprocess would break the safe-path latency budget. Like git, the search stops
where the filesystem changes (`st_dev`), so the workspace is the repository that
git and Snapshot see. Until
[#229](https://github.com/IliasAlmerekov/aegis-shellguard/issues/229) makes the
Sandbox mandatory, Aegis resolves the workspace only when the Sandbox applies.

**2. No enclosing worktree means no workspace.** There is no fallback to the
working directory. Otherwise `cd ~/.ssh` followed by any command would make
`~/.ssh` writable. Without a workspace the default ceiling admits only the
temporary directory. Creating a repository to get around this needs a write that
the ceiling already refuses.

**3. A degenerate root is not a workspace.** A worktree root equal to `/`, to the
user's home, or to an ancestor of the home yields no workspace either. This covers
a dotfiles repository at `$HOME`. The home comes from the passwd entry of the
current uid. The `HOME` variable is used only when the uid has no passwd entry, as
in a container started with an arbitrary uid.

**4. A linked worktree or submodule brings its git directories.** When `.git` is a
file, its `gitdir` and the `commondir` read from that gitdir join the ceiling, so a
commit can write objects and refs. Aegis accepts the gitdir only when it points
back at this worktree: the `gitdir` file of a linked worktree, or `core.worktree`
of a submodule. A `.git` file the agent wrote cannot pass that check, because
forging the back-pointer needs a write inside the target. A gitdir that fails the
check is left out with `trusted_ceiling_path_omitted { reason: gitdir_unverified }`,
and the worktree root stays.

**5. Environment variables do not move the workspace.** `GIT_DIR`,
`GIT_WORK_TREE`, and `GIT_COMMON_DIR` are ignored. The agent sets the
environment, so it is not a trusted input to the ceiling.

**6. The temporary directory comes from the OS, not from `TMPDIR`.** On Linux it is
`/tmp`. On macOS it is `/tmp` plus the per-user temporary directory returned by
`confstr(_CS_DARWIN_USER_TEMP_DIR)`.

**7. A missing workspace is never silent.** It raises a typed
`workspace_unresolved` warning carrying its reason (`no_worktree`, `root`, `home`,
`home_ancestor`) and the working directory. It surfaces where the other ceiling
warnings do: `aegis config validate` and `aegis status`, never `$SHELL -c`. A
computed root (the worktree root, a gitdir, or the temporary directory) that cannot
enter the ceiling is reported as `trusted_ceiling_path_omitted` with the layer
`Computed`, because no config layer supplied it.

**Accepted risk.** The agent can still `cd` into any existing repository the user
owns and write there. Aegis has no session state that would pin the workspace to
the directory the agent started in, and adding one is out of scope for 1.0.

**One exception to ADR-029 invariant E.** A 0.6.x config with `enabled = true` and
no `allow_write` confines writes to zero roots today. Under the computed default
the same file gets the workspace and the temporary directory, so this migration
grants authority. The exception is accepted: an empty default makes a mandatory
layer unusable for ordinary work, and only installations that opted in to the
Sandbox before 1.0 are affected. A config that wants the old posture writes
`allow_write = []`.
