# ADR-030 — The confinement profile is derived from the Assessment

## Status

Accepted. Extends [ADR-029](adr-029-the-sandbox-is-a-mandatory-1-0-layer.md).

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
