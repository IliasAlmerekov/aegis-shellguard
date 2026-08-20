# ADR-029 — The Sandbox is a mandatory 1.0 layer

## Status

Accepted. Supersedes [ADR-003](adr-003-aegis-is-a-heuristic-guardrail-not-a-sandbox.md).

## Context

ADR-003 fixed the original posture: Aegis operates on command text and policy
decisions before the real shell runs, and is "a heuristic guardrail, not a
sandbox". `PRD.md` §2 followed it, describing the OS Sandbox as an optional
reinforcing layer, and the code agreed — `SandboxSettings` derives `Default`, so
`enabled` and `required` are both `false`
(`crates/aegis-config/src/model.rs:222-234`). For most users the layer was not
merely optional; it was absent.

Rebaselining 1.0 scope ([#199](https://github.com/IliasAlmerekov/aegis-shellguard/issues/199))
raised the obvious objection: Codex and Claude Code already sandbox themselves,
so a second confinement layer looks like duplication. Reading the official Codex
source settled it in the other direction.

**An agent sandbox protects the host from the agent, and its permitted region is
the workspace.** Codex's default filesystem policy is `workspace-write`
(`codex-rs/protocol/src/config_types.rs:91`; a trusted directory resolves to
`WorkspaceWrite`, an untrusted one to `ReadOnly`,
`codex-rs/config/src/config_toml.rs:748`). Everything Aegis exists to catch —
`rm -rf ./src`, `git reset --hard`, `git clean -fdx`, destructive SQL against a
reachable database — is inside the permitted region.

**The agent's own judgement is what stands between the workspace and those
commands.** `AskForApproval::OnRequest` is `#[default]`, documented as "The model
decides when to ask the user for approval"
(`codex-rs/protocol/src/protocol.rs:924`). Its built-in danger detection is one
rule on Unix — forced `rm`
(`codex-rs/shell-command/src/command_safety/is_dangerous_command.rs`, 361 lines);
every other program returns `None`. Its `execpolicy` engine ships no default rule
corpus. Aegis has 70+ typed rules and, per
[ADR-026](adr-026-snapshot-rollback-contract-for-1-0.md), a Snapshot/Rollback
contract that Codex has no equivalent of at all.

**Nesting was measured, not assumed.** On Linux, Aegis' bwrap argv runs inside
Codex's verbatim `workspace-write` argv with exit 0; `--cap-drop ALL` does not
block the nested user namespace; an inner `--bind` of a path the outer layer made
read-only still yields "Read-only file system"; and nested Landlock rulesets
compose as an intersection. The inner layer can only tighten. The one blocker was
inside Aegis — Landlock was applied before exec'ing bwrap, which the kernel
forbids — and it is fixed
([#211](https://github.com/IliasAlmerekov/aegis-shellguard/issues/211)).

So the mechanism overlaps; the purpose does not. The agent's sandbox is a
host-protection boundary with one static session-wide profile. Aegis confines each
command according to what its own rule corpus knows that command destroys.

## Decision

Decided in [#207](https://github.com/IliasAlmerekov/aegis-shellguard/issues/207).

**1. The Sandbox is a mandatory layer of Aegis 1.0**, not an optional add-on.
Confinement is attempted for every executed command on Linux and macOS.

**2. `sandbox.required` is removed from the configuration.** A mandatory layer
cannot also be skippable: `required = false` would mean "mandatory, but may be
bypassed", which is the failure mode M1 was raised to fix. Unavailability blocks;
there is no second flag expressing it. *(Amended 2026-08-20: `sandbox.enabled`
goes with it, and the migration contract for both is below.)*

**3. Linux confinement follows the Codex pattern.** Prefer a `bwrap` found on
`PATH`; fall back to bubblewrap built from vendored C sources inside
`aegis-sandbox`; warn at startup when neither is usable; refuse the commands that
would need the unavailable path (the WSL1 case). macOS uses Seatbelt, which ships
with the OS and needs no bundling.

**4. Bubblewrap becomes the second named native-C exception**, after Tree-sitter
([ADR-022](adr-022-language-aware-analysis-is-an-additive-isolated-stage.md) §8).
It is scoped to `aegis-sandbox` at a pinned version. `PRD.md` §6 and `CLAUDE.md`
are amended to name it; the general prohibition on C build steps stands, and this
is not permission for further native dependencies.

**5. Failure policy: always attempt, never silently unconfined.** When the layer
cannot be applied, the command fails with an actionable message; it does not run.
This mirrors Codex, whose sandbox selection is by platform rather than by runtime
probe, so an unavailable layer produces an error rather than a downgrade.

**6. `SandboxStatus` keeps its four values.** The audit format is a public
contract from 1.0 and `Unavailable` remains an accurate description of the fact.
What changes is the consequence: on Linux and macOS, `Unavailable` now always
accompanies `Decision::Blocked`. The M1 active-channel warning survives for
`NotConfigured` and for `Mode::Audit`, where no confinement is expected.

**7. The surviving honesty claim replaces "not a sandbox".** Aegis confines writes
and network per command; **it is not a confidentiality boundary and not a
privilege boundary**. The six banned confidentiality overclaims pinned by
`tests/contracts_docs.rs` stay banned, and no document may promise that file reads
or secrets are hidden from a command.

**8. macOS nesting remains unmeasured and is the one open sub-question.** Nested
`sandbox-exec` under Codex and under Claude Code has not been tested; the operator
script and its pass/fail verdicts are recorded on
[#208](https://github.com/IliasAlmerekov/aegis-shellguard/issues/208). Codex
cannot answer it, because Codex is always the outer sandbox and never nested.
Until it is run, decisions 1–7 hold for Linux, and macOS follows the same failure
policy. If the measurement shows that a nested inner profile can **widen**
authority, this ADR is reopened — that would make an inner layer a
privilege-escalation vector rather than a guardrail.

## Amendment — 2026-08-20: the `[sandbox]` migration contract

Decided in [#240](https://github.com/IliasAlmerekov/aegis-shellguard/issues/240).
Decision 2 above was incomplete rather than wrong: it removed `required` and left
`enabled`, whose `false` value reproduces the identical "mandatory, unless
disabled" hole this ADR was raised to close. `enabled` is not a setting inside the
layer but the constructor of the whole layer —
`config.sandbox.enabled.then(|| SandboxConfig { .. })` (`src/runtime/context.rs:52`)
— so removing `required` alone was cosmetic. The amendment also settles what
happens to config files that already carry either field, which neither this ADR
nor [ADR-030](adr-030-the-confinement-profile-is-derived-from-the-assessment.md)
addressed at all.

**A. Both runtime flags leave the configuration contract.** `sandbox.enabled` and
`sandbox.required` are removed from the 1.0 contract together. The obligation,
stated precisely: confinement is applied outside `Mode::Audit` to every command
that reaches the enforcement flow. `Toggle` and `Disabled passthrough` remain a
separate operator mechanism and are **not** an opt-out of the layer.

**B. Legacy keys are accepted, ignored, and warned — never rewritten.** Both
fields are tolerated by **exact name only**, so every other unknown key stays
rejected by `deny_unknown_fields` and a typo keeps failing. The value is ignored
**whatever it is**: `enabled = false` is a released, valid configuration, and
ignoring it is not fail-open, because the layer applies regardless. Each
occurrence raises a typed `deprecated_sandbox_field` warning carrying its layer
and location, alongside the existing `project_security_ratchet` family, and the
message names `mode = "Audit"` for observe-only use. **The file is not rewritten.**
The existing allowlist auto-migration (`crates/aegis-config/src/model/migration.rs:64`)
is a precedent for in-place rewriting, but a syntactic, meaning-preserving one;
deleting a security field changes meaning, which is exactly why Aegis should not
be the one editing it.

**C. Lifetime: the whole support life of config schema v1.** Removing the shim
requires a separate decision to end schema v1 support. The ground is the
compatibility contract of schema v1 plus the low cost of an exact-name shim — not
the [ADR-026](adr-026-snapshot-rollback-contract-for-1-0.md) legacy `snapshot_id`
precedent, which rests on append-only Audit lines being required for Rollback.
`config_version` was considered as the migration channel and rejected: the field
is optional and defaults to `CURRENT_CONFIG_VERSION`
(`crates/aegis-config/src/model/serde_helpers.rs:38`), so after a bump a
versionless 0.6.x config is indistinguishable from a freshly generated one, and
migration would be guessing.

**D. `allow_write`, path containment, and the empty ceiling.** Absent means the
computed default ceiling; present is an explicit override of it, never an
addition; an explicit `[]` is valid and means zero configured writable roots. A
ceiling emptied by configuration or by omitted entries gets no fallback. A
malformed entry is omitted with a typed `trusted_ceiling_path_omitted` outcome
rather than failing the load. The full per-field matrix, the containment rules,
and the reasons are recorded in the resolution of
[#240](https://github.com/IliasAlmerekov/aegis-shellguard/issues/240); the
normative statement is `PRD.md` §5.5, and
[ADR-030](adr-030-the-confinement-profile-is-derived-from-the-assessment.md)
carries the matching refinement.

**E. Migration may only ever reduce authority, never grant it.** Every rule above
obeys this invariant, and it is the reason an emptied ceiling gets no fallback and
a symlink escape is caught at enforcement rather than trusted at merge.

Consequences of the amendment: [ADR-013](adr-013-project-config-security-ratchet.md)
loses two of its ratcheted fields and gains a semantic-intersection rule for
`allow_write` (annotated there); the removal in the code is owned by
[#229](https://github.com/IliasAlmerekov/aegis-shellguard/issues/229), whose scope
this amendment supersedes; `docs/config-schema.md`, `CONTEXT.md`, and
`aegis-schema.json` follow on
[#205](https://github.com/IliasAlmerekov/aegis-shellguard/issues/205) and
[#242](https://github.com/IliasAlmerekov/aegis-shellguard/issues/242).

## Consequences

- **The 1.0 gate grows.** bwrap + Landlock on Linux and Seatbelt on macOS become
  release-gated on real runners, and all four release targets must build with
  bubblewrap. `libcap` via `pkg-config` is a build input, which is real work for
  the two musl targets — Codex handles it with `PKG_CONFIG_ALLOW_CROSS` and
  `PKG_CONFIG_SYSROOT_DIR`. Unlike ADR-024, this decision moves 1.0 further away;
  that cost is accepted deliberately.
- **A new licence obligation, invisible to the current gate.** Bubblewrap is
  `LGPL-2.0-or-later`, while `CONVENTION.md` permits MIT / Apache-2.0 / ISC and
  `cargo deny` enforces it. Vendored C is not a cargo dependency, so `cargo deny`
  will not see it: the guarantee would be silently hollow exactly where the new
  licence appears. Bubblewrap must be recorded in `THIRD_PARTY_NOTICES.md`, and a
  check has to read vendored sources rather than only the cargo graph.
- **Documentation debt is large and partly test-pinned.** "Aegis is not a sandbox"
  appears in 21+ places, and `docs/release-readiness.md:24` is a **closed `[x]`**
  launch item asserting that README, docs, and release notes agree with it. That
  item reopens. The rewrite is owned by
  [#204](https://github.com/IliasAlmerekov/aegis-shellguard/issues/204) and
  [#205](https://github.com/IliasAlmerekov/aegis-shellguard/issues/205), not by
  this ADR.
- **`CONTEXT.md` must be updated.** The `Sandbox` entry currently reads "optionally
  applied … best-effort write/network guardrail add-on", which this decision
  contradicts. Its "not a security or confidentiality boundary" clause stays.
- **ADR-021 and the M1 contract narrow rather than disappear.** Reporting the
  actual execution path at the seam matters more, not less, once unavailability is
  a block: an audited `Active` that did not confine anything is now a wrong
  statement about a mandatory layer.
- **Binary size grows** on Linux by the bundled bubblewrap, and the default-build
  size figure has to be re-measured.
