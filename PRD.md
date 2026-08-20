# PRD — Aegis 1.0

**Status:** Approved specification for the Aegis 1.0 production release
**Document revision:** 3
**Last updated:** 2026-08-20

This PRD is the single normative source of the Aegis 1.0 product promise.
The `1.0` milestone is the live release gate under
[ADR-027](docs/adr/adr-027-one-1-0-release-gate-lives-in-the-issue-tracker.md).
`ROADMAP.md` records the historical path taken, `docs/release-readiness.md` holds
release evidence, and `TASKS.md` is the historical registry of security findings.
None of them restates the gate.

---

## 1. Product Overview

Aegis is a lightweight Rust CLI that acts as a `$SHELL` proxy. It sits between an
AI agent and the real shell, intercepts every shell command, classifies it by
risk level, and requires human confirmation before destructive operations run.

**Problem.** AI agents (Claude Code, Codex, Cursor) run shell commands fast and
with full permissions. A single bad command deletes files, resets a repository,
drops a database, or pushes something dangerous. The developer has no control
point between the agent's intent and the irreversible action.

**Promise (one-liner).** Aegis is the last barrier between an AI agent and a
dangerous command: safe commands run instantly, risky ones require confirmation,
the worst are always blocked, and every command that does run is confined by the
OS.

---

## 2. Positioning

Aegis 1.0 is a **rule-driven guardrail for AI-agent shell commands**. It is built
from three layers, and their 1.0 status is not uniform:

- **Heuristic decision layer — the core of 1.0.** A fast rule corpus over parsed
  commands decides `Safe` / `Warn` / `Danger` / `Block` and drives the
  confirmation dialog.
- **Snapshot / Rollback — mandatory in 1.0.** All six providers are part of the
  promise and are proven by a CI snapshot → rollback cycle
  ([ADR-026](docs/adr/adr-026-snapshot-rollback-contract-for-1-0.md)). Recovery is
  an obligation, not a convenience, wherever no human made the decision (§5.10).
- **Sandbox — a mandatory 1.0 layer.** Confinement is attempted for every
  executed command; failure to establish it blocks execution rather than
  downgrading to an unconfined run
  ([ADR-029](docs/adr/adr-029-the-sandbox-is-a-mandatory-1-0-layer.md)).
- **Language-aware analysis — not in 1.0.** It ships after 1.0 behind an opt-in
  cargo feature and is an explicit 1.0 Non-Goal; see §5.11 and §11.

**Relationship to the agent's own sandbox.** An agent's sandbox protects the host
*from* the agent, and the region it permits is the workspace itself — everything
inside that region is granted wholesale. Aegis confines what the agent does
*inside* that permitted region, driven by its own rule corpus, which knows which
commands destroy what. The two layers are therefore complementary rather than
duplicative: the outer one decides where the agent may act at all, the inner one
narrows each individual command within it. Nesting was measured on Linux and can
only ever tighten authority, never widen it
([#208](https://github.com/IliasAlmerekov/aegis-shellguard/issues/208)).

**Honest boundaries (must be reflected in the product and docs).** Aegis confines
writes and network access per command; it is **not a confidentiality boundary and
not a privilege boundary**. No document may promise that file reads or secrets are
hidden from a command. Aegis is also **not** a guarantee against malicious code:
the heuristic layer does not catch

- obfuscated or encoded commands,
- `eval "$(...)"` and commands assembled at runtime,
- indirect execution (write a script first, run it later).

Indirect execution is instead handled as `Effect-opaque execution` under the
Recovery obligation of §5.10 rather than by inspecting the referenced script.
These limitations are documented in `docs/threat-model.md` and must be visible in
the README.

---

## 3. Target Audience

**Primary (1.0 focus).** Individual developers running AI agents locally — from
"vibe coders" to experienced engineers who give the agent full shell access.
Priorities for this audience:

- one-command install that works out of the box (zero-config by default),
- no friction on the safe hot path (< 2 ms),
- clear prompts and learnability (repeated decisions are not re-asked).

**Secondary (post-1.0, considered architecturally but out of release scope).**
Teams and organizations: shared policies, centralized audit for compliance. The
architecture (`[[rules]]`, audit trail, config layering) must not foreclose this
path, but team features are not shipped in 1.0.

---

## 4. User Scenarios

1. **Intercept a dangerous command.** The agent runs `rm -rf ./src`. Aegis
   recognizes `Danger`, shows a TUI dialog with `justification`, and offers
   `[A]llow / [D]eny / [Always allow] / [Always deny]`.
2. **Block the worst.** The agent runs `rm -rf /`. Aegis always refuses, with no
   confirmation option.
3. **Learning via persistence.** The user picks "Always allow" — the decision is
   automatically written to config as a typed rule; the same command is no longer
   re-prompted.
4. **Snapshot and rollback.** Before an approved destructive command, Aegis takes
   a snapshot via an applicable provider; if needed, the user rolls back with
   `aegis rollback '<snapshot-id>'`. When no provider applies, that fact is
   disclosed rather than passed over in silence.
5. **Recovery on an unattended run.** In CI, an auto-approved destructive command
   has no human to warn. Aegis re-checks Recovery readiness on every such run and
   halts the command when the obligation cannot be met (§5.10).
6. **Confined execution.** Every executed command runs inside an OS confinement
   whose write and network authority is bounded by the `Trusted ceiling` and
   narrowed further by whichever rules matched the command.
7. **Audit.** Every decision and every execution is written to an append-only
   JSONL log; the user reviews history and verifies the chain integrity
   (`aegis audit --verify-integrity`).
8. **Temporary disable.** `aegis off` / `aegis on` / `aegis status`; in a
   detected CI environment, policy stays enforced by default.

---

## 5. Functional Requirements 1.0

### 5.1 Interception and classification

- Intercept every shell command in the `$SHELL` proxy role and in `aegis -c` mode.
- Classify by `RiskLevel`: `Safe`, `Warn`, `Danger`, `Block` (the order is
  semantically ordered by severity and does not change).
- Commands are parsed into logical scan targets and evaluated by the built-in
  token-prefix and regex rule corpus. The original command text is retained for
  user-visible explanation and Audit. The index that makes this fast is an
  implementation concern and is deliberately not specified here; the observable
  requirement is the hot-path budget in §6.
- Support for `Alts` (semantic flag equivalents in one rule), `justification`,
  `match_examples` / `not_match_examples`.
- Built-in rules (≥70) are validated against their own examples in debug/tests.

### 5.2 Policy DSL

- **The typed TOML DSL is the only way to declare a Policy rule in 1.0.** There is
  no second, scripted authoring path
  ([ADR-028](docs/adr/adr-028-the-starlark-policy-dsl-is-removed-before-1-0.md)).
  A `~/.aegis/policy.star` left over from a pre-1.0 installation is a permanent
  startup error — never a warning, never a silent ignore.
- `[[rules]]` fields: `pattern` with `Alts`, `decision`
  (`allow`/`prompt`/`block`), `justification`, `match_examples`,
  `not_match_examples`, a `when` clause (environment-conditional decision), and an
  optional `Confinement restriction` (§5.5).
- Rules are validated at load time. Invalid rules fail configuration loading with
  a human-readable validation error.

### 5.3 Decision persistence

- "Always allow" appends an `[[allow]]` rule to the active config; "Always deny"
  appends a `[[block]]` rule.
- The command is tokenized into a prefix (program + meaningful flags; variable
  arguments are stripped).
- Deduplication on write: a duplicate is skipped silently; a conflict (same
  pattern, different decision) emits a warning with the existing rule's location.
- The scanner cache is invalidated; the new rule takes effect immediately.
- The legacy `allowlist` is migrated to `[[allow]]` with a deprecation warning.

### 5.4 Snapshot / rollback

This section defines the Snapshot machinery. **When a Snapshot is an obligation
rather than a best effort is defined in §5.10**, which holds the single policy
invariant; nothing here overrides it.

- `SnapshotPlugin` trait (async, via `async-trait`) + 6 providers: Git, Docker,
  PostgreSQL, MySQL/MariaDB, SQLite, Supabase. **There is no provider tiering** —
  all six are part of the 1.0 promise and each must be proven by a CI
  snapshot → rollback cycle, Supabase through its Postgres compatibility plus one
  live pre-release run
  ([ADR-026](docs/adr/adr-026-snapshot-rollback-contract-for-1-0.md)).
- A Snapshot is taken **only when the command is approved (`Allow`)** — never for
  `Block`ed commands — and runs **before** confined execution, so a rollback
  target exists independently of the confinement layer.
- **Silent degradation ends.** When no provider applies to the command, that is
  disclosed in the confirmation dialog in red and requires the full word `yes`;
  it is never passed over.
- `snapshot_id` is `v3`, using `:` as its separator. Legacy `v2` tab-joined ids
  stay parseable permanently.
- `aegis rollback '<snapshot-id>'` restores state, behind a preflight that
  distinguishes "artifact absent" from "check inconclusive". With no TTY,
  `rollback` fails closed.
- `aegis snapshot list` enumerates available snapshots with their `snapshot_id`,
  provider, and creation time so the opaque id is discoverable.
  `aegis snapshot verify` is the separate, uncached liveness check; liveness is
  never folded into the pre-command path.
- **Lifecycle:** snapshots are subject to a configurable retention policy
  (by count and/or age) under `[prune]`; `aegis snapshot prune` removes snapshots
  beyond the retention bound. Retention applies across providers (git stashes,
  Docker images, SQLite/PostgreSQL/MySQL dumps) to bound unlimited growth.
- No blocking I/O in async context (`tokio::time::sleep`, no `spawn_blocking`
  workarounds).

### 5.5 Sandbox

The Sandbox is a **mandatory Sandbox** layer of Aegis 1.0, not an add-on
([ADR-029](docs/adr/adr-029-the-sandbox-is-a-mandatory-1-0-layer.md)), and the
profile it applies is derived per command rather than fixed for the session
([ADR-030](docs/adr/adr-030-the-confinement-profile-is-derived-from-the-assessment.md)).

**Obligation.** Confinement is applied to every command Aegis executes outside
`Mode::Audit`. Failure to establish the required confinement **blocks execution**;
a command never runs silently unconfined, and there is no configuration flag that
turns unavailability back into a fallback. Invalid profiles and unexpected setup
errors remain fail-closed.

**Platform mechanisms.**

- **Linux:** bubblewrap plus Landlock (LSM) for defense in depth. Aegis prefers a
  `bwrap` found on `PATH` and otherwise falls back to bubblewrap built from
  vendored C sources; it warns at startup when neither is usable and refuses the
  commands that would need the unavailable path.
- **macOS:** Seatbelt via `/usr/bin/sandbox-exec` with a `.sbpl` profile.
- **Windows:** not supported. On Windows, Aegis runs only inside WSL2, where it is
  a Linux environment and uses the Linux mechanism (§8).

**The `Trusted ceiling`.** The statically configured profile is the upper bound on
what any command may be granted, never a command's final profile. Its default is a
working one, because a mandatory layer whose default forbids all writes is
unusable: writes within the workspace tree and `/tmp`, with network access
disabled. A `cwd` of `/` does not implicitly make `/` a workspace. The project
config layer may only narrow the ceiling, never widen it.

**Derivation only ever subtracts, and only where a rule fired.** The effective
profile is the `Trusted ceiling` intersected with the project tightening, with the
restrictions carried by the rules that matched, and with any outer agent sandbox.
Consequences of that shape, all of them normative:

- An empty `matched` yields the ceiling unchanged, so the safe hot path performs
  no derivation work and stays inside the §6 budget.
- A rule may carry an optional typed `Confinement restriction`. `RiskLevel` and
  `Category` take no part in derivation, and a restriction is never expressed as
  an argv index: a wrong profile here is a broken command with no dialog to catch
  it.
- No derived profile may grant a path, a network permission, or any other
  authority the ceiling withholds.
- When a restriction is declared but its extractor resolves no target, the command
  runs under the ceiling and a `Confinement degradation` is recorded. The
  degradation is **visible in the confirmation dialog**, not only in the Audit log,
  and it never blocks: blocking would turn a parser gap into an outage.

**Observability.** The effective profile is shown in the confirmation dialog and
recorded in the Audit log as a new field. When the layer cannot be established the
Audit records `sandbox_status = "unavailable" accompanies Decision::Blocked`,
because under a mandatory layer those two facts are one event rather than two.
`SandboxStatus` keeps its four values — the Audit format is a public contract from
1.0 and `Unavailable` remains an accurate description of the fact; only its
consequence changed. The active-channel warning survives for `NotConfigured` and
for `Mode::Audit`, where no confinement is expected.

**Boundaries.** The Sandbox is a write/network guardrail. It is
**not a confidentiality boundary and not a privilege boundary**: profiles do not
promise to hide readable files or secrets from a command.

**Configuration.** An existing `[sandbox]` section keeps loading under the
mandatory layer, and the two flags that could switch the layer off leave the 1.0
configuration contract. One invariant governs every field below: migration may
only ever reduce authority, never grant it.

- **`sandbox.enabled` and `sandbox.required` are not part of the 1.0
  configuration contract.** A mandatory layer cannot also be skippable, so no
  flag expresses "mandatory, but may be bypassed". Both are still accepted **by
  exact name and ignored at whatever value they carry**, for the whole support
  life of config schema v1, and each raises a typed `deprecated_sandbox_field`
  warning naming its layer and location. `enabled = false` is not fail-open: the
  layer applies regardless. Observe-only operation is `mode = "Audit"`, which is
  where the obligation itself stops. `Toggle` and `Disabled passthrough` stay a
  separate operator mechanism and are not an opt-out of the layer.
- **Aegis never rewrites a config file to migrate it.** Deleting a security field
  changes what the file means, so the record of what an installation once asked
  for stays where its owner put it.
- Every other unrecognised `[sandbox]` key is still rejected, so a typo keeps
  failing the load rather than being silently tolerated.
- **`sandbox.allow_write` is an explicit override of the computed default
  ceiling, never an addition to it.** Absent, the ceiling is the computed default
  above; present, the listed roots are the whole configured set; an explicit `[]`
  is valid and means **zero configured writable roots**. A ceiling emptied by
  configuration or by omitted entries gets **no fallback** to the computed
  default — a fallback would be the one place Aegis grants authority the config
  never asked for. At the project layer the list is a semantic intersection of
  path trees with the trusted base, and an attempted widening keeps the base and
  warns.
- A ceiling entry that is relative, contains `..`, does not exist, or resolves
  canonically outside the ceiling is **omitted from the effective ceiling** with a
  typed `trusted_ceiling_path_omitted` outcome carrying its reason
  (`relative` / `parent_dir` / `not_found` / `outside_trusted_ceiling`), its
  layer, and its location. Authority only narrows. This is deliberately not a
  `Confinement degradation`, which names the opposite movement — a profile that
  widened to the ceiling. Absolute paths only; `.` may be dropped
  component-wise; `..` is never folded lexically, because `/workspace/link/../secret`
  with `link -> /etc/subdir` folds to a path the filesystem does not resolve to.
  Containment is therefore checked at two moments: component-wise at merge, with
  no filesystem access, so a ceiling path that does not exist yet is not a startup
  error; and canonically at enforcement, on **both** the trusted and the effective
  roots.
- `sandbox.allow_network` is unchanged by the mandatory layer: absent is `false`,
  the global layer is last-wins, and the project layer may only tighten.

The omission promise is bounded rather than absolute: a malformed ceiling entry
never blocks the shell, but the command can still fail on write under the
narrower profile, and a path that disappears between canonicalisation and
`bwrap --bind` fails setup.

**Where the diagnostics surface.** `deprecated_sandbox_field` and
`trusted_ceiling_path_omitted` are typed warnings, not startup logs: a
per-invocation warning on the `$SHELL -c` path would be noise and a risk to the
Hook protocols. `aegis config validate` is the authoritative surface and
`aegis status` an ordinary discovery surface for the same warnings; neither
changes its exit code on warnings alone. The promise is that the warning is
**available** on a discovery surface, not that every user reads it.

The Audit log records the effective profile that was granted (§5.5,
Observability) and not the causal diagnostics of how it was built. Config and
runtime diagnostics own the reason; the Audit log is not a history of config
provenance.

### 5.6 Audit log

- Append-only JSONL at `~/.aegis/audit.jsonl`; the file is only appended to.
- Each entry is an `AuditEntry` (typed enum: `Decision` / `Watch`).
- A `sandbox_status` field (`active` / `unavailable` / `not_configured` /
  `not_attempted`); the legacy `sandbox_active` boolean is mirrored where its
  older tri-state can represent the status.
- Audit integrity chain: SHA-256 hash chain, mode `ChainSha256` enabled **by default**
  (opt-out, not opt-in).
- **Concurrent writes:** appends are serialized with an advisory file lock
  (`flock`) so parallel Aegis processes (multiple agent sessions) cannot interleave
  entries and break the hash chain. The lock is held only for the duration of a
  single append.
- Any audit write failure is a hard error with a non-zero exit code, regardless
  of `verbose`.
- The log format is part of the public contract from 1.0.

### 5.7 Toggle and CI contract

- `aegis on` / `aegis off` / `aegis status` (global `~/.aegis/disabled` flag).
- In disabled mode outside CI, Aegis behaves as if it is not installed
  (zero-noise) while preserving the toggle history.
- In a detected CI environment, policy stays enforced by default; `AEGIS_CI`
  explicitly overrides CI detection in either direction.

### 5.8 Agent integrations

- **First-class:** Claude Code and Codex — via hook integration
  (`aegis install-hooks --all`), including the shared toggle helper.
- Other agents (Cursor and anything that respects `$SHELL`) work via `$SHELL` on
  a best-effort basis; documented, but not first-class.
- Hook installation is binary-first: it updates existing `~/.claude` / `~/.codex`
  directories and skips missing ones without creating them.

### 5.9 Configuration

- TOML: `~/.config/aegis/config.toml` (global) and `.aegis.toml` (per-project),
  with layered merge.
- All fields are optional with defaults via `#[serde(default)]`; backward
  compatibility with existing config files is not broken. The `[sandbox]`
  section is the worked case: a released config keeps loading even though two of
  its fields left the contract (§5.5).
- `aegis config init|show|validate`; a JSON schema is generated from the type for
  editor autocompletion.

### 5.10 Recovery

Recovery is the ability to undo an executed destructive command. The **obligation**
to have it attaches to the absence of a human decision, not to `RiskLevel`
([ADR-031](docs/adr/adr-031-unattended-destructive-execution-requires-recovery.md)).
This section holds that invariant; §5.4 holds the machinery.

#### Attended `Danger`

A `Danger` command a human confirmed in the dialog carries a deliberately weaker
promise, stated honestly rather than dressed up:

- A Snapshot is taken when a provider applies to the command.
- When no provider applies, the absence of Recovery is disclosed in the dialog
  (§5.4) rather than passed over.
- The Sandbox bounds the blast radius of what was confirmed (§5.5).
- **A mistakenly confirmed deletion with no applicable provider can still be
  irreversible.** 1.0 does not promise otherwise.

#### `Unattended destructive execution`

When no human decided — an auto-approved `Danger` command, or
`Effect-opaque execution` where Aegis cannot see what will run — Recovery becomes
a precondition of execution:

- Recovery readiness is re-checked on **every** unattended run. A persisted
  approval is not a persisted permission to run without Recovery.
- An unsatisfiable obligation **halts** the command. It does not evaporate into a
  warning, and it does not degrade into an unprotected run.
- The halted path offers the `Recovery override` dialog, which grants a
  **one-time Recovery override** for a single run and is never persisted.
- `Mode::Audit` and a trusted `snapshot_policy = None` remain the two `Recovery
  opt-out`s. Both stop being silent.
- **A persisted `Allow` rule for a `Danger` command no longer guarantees execution
  in CI.** This is a deliberate behavioural change: configurations that opted out
  of the dialog now pay a per-run readiness gate.
- Readiness verification is a cheap local artifact check — bounded at ≤ 100 ms per
  attempt and ≤ 500 ms per command, with no network access. It is never the §5.4
  liveness check, and a missing checking tool yields a presence-only readiness
  rather than an invalid one.

### 5.11 Language-aware analysis is a 1.0 Non-Goal

Language-aware analysis — parsing the *contents* of scripts and source files to
reason about what they do — is **explicitly not part of 1.0**
([ADR-022](docs/adr/adr-022-language-aware-analysis-is-an-additive-isolated-stage.md),
[ADR-024](docs/adr/adr-024-language-aware-analysis-ships-opt-in-and-is-not-a-1-0-release-gate.md)).
It ships after 1.0 behind the `language-analysis` cargo feature, default off, and
the Tree-sitter grammars are absent from the 1.0 release binaries. The target is
"after 1.0" with no version number attached; finishing the qualification matrix
early does not return it to 1.0, because the grounds are scope rather than
schedule.

The type surface that anticipates it — `aegis-types::analysis` and the
`[language_analysis]` config section — stays unconditional, so enabling the feature
later is additive.

This document deliberately makes **no** functional promise about analysing file
contents anywhere in §5, §9, or §10. That silence is correct and must not be
"fixed" by adding language-aware analysis as a 1.0 feature.

---

## 6. Non-Functional Requirements

- **Performance:** safe hot path < 2 ms (p99). Any change to `scanner`/`parser`
  is benchmarked with `cargo criterion`; regressions are not allowed.
- **Parsing correctness:** the parser is a security-critical input; fuzzing is
  mandatory (parser, scanner, heredoc unwrapping).
- **Dependency security:** `cargo audit` and `cargo deny check` pass with zero
  findings; permissive licenses only (MIT/Apache-2.0/ISC); no duplicate core
  crates and no banned crates.
- **Portability:** no dependencies with a C build step; a statically portable
  binary. There are exactly **two named exceptions**, both narrowly scoped: the
  pinned Tree-sitter runtime and its production-qualified generated grammars,
  confined to `aegis-language`
  ([ADR-022](docs/adr/adr-022-language-aware-analysis-is-an-additive-isolated-stage.md)
  §8), and **bubblewrap**, built from vendored C sources at a pinned version and
  confined to `aegis-sandbox`
  ([ADR-029](docs/adr/adr-029-the-sandbox-is-a-mandatory-1-0-layer.md)). The
  general prohibition stands; neither exception is permission for further native
  dependencies.
- **Third-party licence obligations:** bubblewrap is `LGPL-2.0-or-later`, which
  `cargo deny` cannot see because vendored C is not a cargo dependency. It must be
  recorded in `THIRD_PARTY_NOTICES.md`, and the licence check must read vendored
  sources rather than only the cargo graph.
- **Architecture:** edition 2024, MSRV `1.80`, no file in `src/` exceeds 800 LoC,
  the crate dependency DAG is enforced by `tests/architecture_boundaries.rs`.
- **Code:** no `.unwrap()`/`.expect()` in production paths; typed errors
  (`thiserror`) in libraries, `anyhow` in bin glue; libraries never write to
  stdout (only `tracing`).

---

## 7. Distribution

Officially supported 1.0 channels:

1. **curl | sh** — convenience installer (global-first), verifying the checksum
   before writing the binary.
2. **GitHub Releases** — prebuilt binaries with `.sha256` sidecars for all
   supported targets.
3. **Homebrew** — official formula/tap for macOS and Linux.
4. **npm** — a wrapper package that downloads and installs the platform binary
   (for the audience used to `npm i -g`).
5. **cargo install** — build from source as a fallback for platforms without a
   prebuilt binary.

---

## 8. Platforms

| Platform               | Shell proxy  | Sandbox                         |
| ---------------------- | ------------ | ------------------------------- |
| Linux x86_64           | ✅           | bubblewrap + Landlock           |
| Linux aarch64          | ✅           | bubblewrap + Landlock           |
| macOS arm64            | ✅           | Seatbelt (`sandbox-exec`)       |
| macOS x86_64           | ✅           | Seatbelt (`sandbox-exec`)       |
| Windows (WSL2)         | ✅ (Linux)   | bubblewrap + Landlock (Linux)   |
| Windows (WSL1)         | ❌           | cannot be established           |

- Native Windows is **not** supported. Native Windows shells (PowerShell,
  cmd.exe) do not work; Aegis runs on Windows only inside WSL2, where it is a
  Linux environment and uses the Linux confinement mechanism.
- **WSL1 cannot establish the mandatory layer**, so Aegis refuses to execute
  commands there rather than running them unconfined.
- Automatic shell setup recognizes `bash` and `zsh`; others via `AEGIS_SHELL_RC`.
- Sandbox unavailability is recorded in the Audit log **and blocks the
  execution-eligible command** (§5.5). There is no unconfined fallback on any
  supported platform.
- macOS nesting under an outer agent sandbox is not yet measured. Should a nested
  inner profile turn out to be able to *widen* authority,
  [ADR-029](docs/adr/adr-029-the-sandbox-is-a-mandatory-1-0-layer.md) reopens —
  that would make the inner layer a privilege-escalation vector rather than a
  guardrail.

---

## 9. Success Metrics

### Technical (quality)

- **Zero false negatives** on the bypass corpus
  (`tests/fixtures/security_bypass_corpus.toml`): no dangerous command from the
  corpus is classified as `Safe`.
- **Hot path < 2 ms (p99)** on safe commands, confirmed by `cargo criterion`.
- **0 CVEs** in dependencies (`cargo audit`) and a clean `cargo deny check`.
- **Green CI on all supported platforms:** Linux (x86_64/aarch64) and macOS
  (arm64/x86_64). Windows is covered transitively via the Linux target (WSL2).

### Security (impact)

- **Dangerous-pattern coverage:** every built-in pattern has ≥1 positive and ≥1
  negative test; all `RiskLevel` variants are covered both ways.
- **Mandatory Sandbox establishment failure rate:** the share of
  execution-eligible commands refused because the mandatory Sandbox could not be
  established. The denominator excludes `Block` and `Denied` commands — they never
  reach the layer. **Target: 0%.** The pre-1.0 "share of commands executed in the
  sandbox" is retired: a successfully executed command outside `Mode::Audit` has
  `SandboxStatus::Active` by definition, so that figure is now identically 100%.
- **Confinement degradation rate:** the share of executed commands that fell back
  to the `Trusted ceiling` because a declared restriction's extractor resolved no
  target (§5.5). Non-zero is expected; a rising trend indicates extractor gaps.
- **Prevented incidents:** the count of `Block` and `Deny` on `Danger` commands
  in the audit log as an indicator of real protection.

---

## 10. Release Gate

**The criterion.** A finding blocks 1.0 **if and only if it falsifies something
this document promises.** The `C` / `H` / `M` / `P3` severity prefix carries no
release weight, and neither does the phase a finding was discovered in.

**The live gate is the `1.0` milestone on the issue tracker.** Every blocking item
is an issue in that milestone; implementation order is expressed as native
blocked-by relationships between those issues. The gate is passed when the
milestone holds no open issues
([ADR-027](docs/adr/adr-027-one-1-0-release-gate-lives-in-the-issue-tracker.md)).

This section deliberately carries **no checklist**. The promises 1.0 must satisfy
are stated normatively in §5 through §8; restating them here would create a second
expression of the same set, which drifts from the first. Anything that reads like a
release checklist elsewhere in the repository is evidence of work done, not a gate.

---

## 11. Out of Scope for 1.0 (Non-Goals)

- Team/centralized mode: shared server-side policies, multi-user management,
  centralized audit collection.
- Native Windows shells (PowerShell, cmd.exe) — WSL2 only; WSL1 refuses execution
  (§8).
- **Language-aware analysis** of file and script contents — see §5.11. It ships
  after 1.0 behind an opt-in cargo feature.
- **A scripted Policy DSL.** The typed TOML DSL is the only rule-authoring path in
  1.0 (§5.2).
- Protection against obfuscation, encoding, and runtime `eval` of commands —
  beyond the heuristic model (documented in the threat model). Indirect execution
  is covered by the Recovery obligation of §5.10, not by analysing what will run.
- **Confidentiality and privilege guarantees.** The Sandbox confines writes and
  network access; it does not hide readable files or secrets, and it is not a
  privilege boundary (§5.5).
- Full shell evaluation and deferred execution
  ([ADR-010](docs/adr/adr-010-full-shell-evaluation-and-deferred-execution-remain-non-goals.md)).
- SBOM, provenance metadata, and attestations in the release workflow.
- A guarantee of byte-for-byte reproducible builds across all environments.
- First-class integration with agents other than Claude Code and Codex (others
  go through `$SHELL`, best-effort).
