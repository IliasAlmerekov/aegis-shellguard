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

**8. macOS nesting was measured and did not reopen this ADR.** *(Originally: "macOS
nesting remains unmeasured and is the one open sub-question." Retired by the
measurement below — recorded as tested-and-not-observed, not deleted, since the
reopen trigger it names is still live for any future macOS Seatbelt change.)*
Nested `sandbox-exec` under a live Codex session and under a live Claude Code
session was measured on macOS 26.5.1 arm64
([#256](https://github.com/IliasAlmerekov/aegis-shellguard/issues/256)): a nested
`sandbox_apply` is refused — `sandbox_apply: Operation not permitted` — on the
first nested call, in both agents, whenever the *outer* process is already under
an active Seatbelt profile. No `INNER_WIDENS_EXIT=0` occurred anywhere, so the
reopen trigger below did not fire and decisions 1–7 stand unchanged for macOS.
The mechanism is ordinary macOS nesting refusal, not an agent-specific behaviour:
Aegis makes exactly one `sandbox_apply` call per command, so it is refused only
when that single call is itself already nested — i.e. only when the shell Aegis
runs as is already confined before Aegis gets to run. Whether that is true is
asymmetric across agents: Codex requires its own sandbox to launch at all, so
every Codex-on-macOS session hits this unconditionally; Claude Code ships with
its sandbox off by default and refuses only once a user opts in via `/sandbox`
(confirmed empirically: toggling `/sandbox` off and on reproduces
`NESTED_SEATBELT_EXIT` flipping 0↔71 with no other change). The consequence for
1.0 shippability is decided in the amendment below. If a future measurement shows
that a nested inner profile can **widen** authority, this ADR is still reopened —
that would make an inner layer a privilege-escalation vector rather than a
guardrail — but that is not what was observed.

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

## Amendment — 2026-08-28: macOS nested-under-active-outer-sandbox ships as decision 5's ordinary consequence

Decided in [#262](https://github.com/IliasAlmerekov/aegis-shellguard/issues/262),
closing the sub-question decision 8 left open. The measurement
([#256](https://github.com/IliasAlmerekov/aegis-shellguard/issues/256)) showed
that on macOS, whenever the shell Aegis runs as is already confined by an active
outer Seatbelt profile — always true under Codex, true under Claude Code only
once a user enables `/sandbox` — Aegis' own single `sandbox_apply` call is itself
a refused nested call, so the layer reports `Unavailable` and every command is
blocked (`SandboxError::Required` → `Decision::Blocked`, `src/shell_flow.rs:414`).

**F. This is shippable for 1.0 as-is; no macOS-specific carve-out is added.**
Treating the outer agent's own confinement as satisfying Aegis' obligation was
considered and rejected: decision 5 ("always attempt, never silently
unconfined") and the positioning from Context above — the agent sandbox is a
host-protection boundary with one static session-wide profile, Aegis confines
each command against its own rule corpus — are exactly why the two layers are
complementary rather than interchangeable. Silently accepting the outer profile
as a substitute on this one platform would be the fail-open this ADR exists to
forbid, applied selectively to whichever host makes it inconvenient. A hard block
is the honest, already-decided consequence of a mandatory layer meeting a
platform where it is provably unobtainable — not a new failure mode invented for
macOS.

**G. The diagnostic must name the cause.** `sandbox_available_for`
(`crates/aegis-sandbox/src/macos.rs:10-24`) already probes by executing the real
per-command profile, so the fact that failure means "already nested under an
active outer Seatbelt profile" is known at the point of failure; today's message
(`SANDBOX_REQUIRED_UNAVAILABLE_MESSAGE`, `"Required Sandbox unavailable; command
not executed."`) does not surface it and reads identically to "no `sandbox-exec`
binary at all". The message must distinguish "nested under an already-active
outer sandbox" from a generic missing/broken `sandbox-exec`, and name the
practical remedy where one exists (e.g. disabling `/sandbox` under Claude Code).
Execution is
[#263](https://github.com/IliasAlmerekov/aegis-shellguard/issues/263).

**H. No asymmetric treatment of Codex vs. Claude Code.** Codex hitting this on
every macOS session while Claude Code hits it only when a user opts in via
`/sandbox` is a fact about each agent's own defaults, not a reason to special-case
either one in Aegis: the failure mechanism, the message, and decision F are the
same regardless of which agent produced the pre-existing outer confinement.

Also surfaced, not part of this decision: the non-required code path in
`src/shell_flow.rs:363-365` still emits pre-mandatory-layer wording ("Sandbox
unavailable; proceeding without confinement. Set sandbox.required = true to block
execution.") — a message from before `sandbox.required` was removed
([#240](https://github.com/IliasAlmerekov/aegis-shellguard/issues/240)) that no
longer describes a reachable configuration. Filed as
[#263](https://github.com/IliasAlmerekov/aegis-shellguard/issues/263) alongside G.

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
