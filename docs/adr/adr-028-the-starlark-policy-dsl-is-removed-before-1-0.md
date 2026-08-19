# ADR-028 — The Starlark policy DSL is removed before 1.0

## Status

Accepted

## Context

Aegis has shipped two ways to declare a Policy rule. The first is the typed TOML
DSL — `[[rules]]` entries with `pattern`, `Alts`, a `PolicyRuleDecision`,
`justification`, `when`, `match_examples`, and `not_match_examples`, validated at
load time with line numbers on failure. The second is `~/.aegis/policy.star`,
evaluated by `crates/aegis-starlark` behind the opt-in cargo feature
`starlark-policy`.

Four facts about the second path were established while rebaselining 1.0 scope
([#222](https://github.com/IliasAlmerekov/aegis-shellguard/issues/222)).

**It expresses nothing the TOML DSL cannot.** `aegis_builtins`
(`crates/aegis-starlark/src/lib.rs:239`) registers exactly one global:
`prefix_rule(...)`. It constructs a `PolicyRule` — the same type a `[[rules]]`
entry constructs, validated by the same
`aegis_config::validate::validate_policy_rules`. The only capability
Starlark adds over TOML is generating rules with loops and functions.

**`PRD.md` promises a second global that does not exist.** `PRD.md:111` names
"`prefix_rule(...)` and `on_command(cmd)`". `on_command` appears nowhere in the
workspace outside that sentence and a `ROADMAP.md` sketch. The half of the
Starlark promise that would have justified a scripting language — reacting to a
whole `Command` rather than declaring a prefix — was never built. This was first
catalogued as a `PRD.md`/code divergence by
[#203](https://github.com/IliasAlmerekov/aegis-shellguard/issues/203).

**It carries four unmaintained advisories.** `starlark 0.14.2` reaches
RUSTSEC-2023-0089, RUSTSEC-2024-0388, RUSTSEC-2025-0057, and RUSTSEC-2024-0436
through `atomic-polyfill`, `derivative`, `fxhash`, and `paste`. They are absent
from the default-feature graph, which is why `cargo deny check` is green and
`cargo audit` exits 0 — the exception is documented in `deny.toml:1-5` and
`docs/release-readiness.md:145-172`, and was registered as finding `P3-7`.
[#202](https://github.com/IliasAlmerekov/aegis-shellguard/issues/202) ruled that
`P3-7` does not block 1.0 on its own, leaving the feature's fate to be decided
on the merits rather than on the advisories.

**CI never compiles it.** `starlark` does not appear anywhere in
`.github/workflows/`. `cargo test --workspace` builds `crates/aegis-starlark` and
runs its 298 lines of tests, but the integration branch in
`src/runtime/context.rs:172-181` sits under `#[cfg(feature = "starlark-policy")]`
and is compiled by no CI job. Only the fail-closed branch — the one that errors
when the feature is off — is ever exercised. `CONVENTION.md:80-81` separately
records that the crate is asserted by neither `tests/platform_scope.rs` nor
`tests/aegis_language_boundary.rs`.

The comparison that settles it is with language-aware analysis. ADR-024 kept L1
in the tree behind `language-analysis` because L1 is a capability nothing else
provides, with eight preserved re-entry conditions. Starlark is sugar over a
capability the tree already ships twice.

## Decision

Decided in
[#222](https://github.com/IliasAlmerekov/aegis-shellguard/issues/222).

**1. The Starlark policy DSL is not part of the 1.0 promise, and the code is
deleted rather than gated.** `crates/aegis-starlark` leaves the workspace and the
`starlark-policy` feature leaves `Cargo.toml`. Deletion is chosen over ADR-024's
feature-gated shape because of decision 4 below: a feature that stays in the tree
now owes CI time, and Starlark would be buying that time with no capability.

**2. The typed TOML DSL is the only way to declare a Policy rule in 1.0.**
`PRD.md` §5.2 loses its Starlark bullet and gains a sentence it can be held to:

> The typed TOML DSL is the **only** way to define a Policy rule in 1.0. Aegis
> ships no scripting language for policy. A `~/.aegis/policy.star` file is a
> startup error, not a silently ignored file.

**3. A `~/.aegis/policy.star` file remains a startup error, permanently.** The
`AegisError::Config` raised from `RuntimeContext` survives the removal with
reworded text directing the user to `[[rules]]`. It is not softened to a
deprecation window and never becomes a silent ignore: a user who wrote that file
believes it is their security policy, and honouring it as a no-op is the one
outcome Aegis must not produce. Published release binaries already behave this
way (`docs/release-readiness.md:158-167`), so no shipped binary changes
behaviour — only the message changes. `tests/starlark_feature_gate.rs` is
rewritten to hold the PRD sentence directly.

**4. A cargo feature that lives in the tree at 1.0 must be compiled and tested by
CI.** "Opt-in" describes what a user chooses to build, not what the project
declines to verify. A feature CI never compiles is code whose breakage reaches
the user before it reaches the maintainer. This applies to every feature, so
`language-analysis` (ADR-024) needs the same CI coverage; that work is tracked
separately and is what makes ADR-024's opt-in shape mean something.

**5. `P3-7` is closed as eliminated, not as accepted.** The advisories do not
become a bounded standing waiver — they cease to exist, because the dependency
chain that reached them is gone. `deny.toml`'s Starlark paragraph and the
`docs/release-readiness.md` "Release binary behavior (policy.star)" section are
deleted rather than reworded.

**Re-entry conditions.** The Starlark DSL returns only when both hold: a
maintained Starlark implementation for Rust exists whose dependency chain is
advisory-free, **and** a policy capability is wanted that the typed TOML DSL
cannot express — `on_command(cmd)`, reacting to a whole `Command` rather than
declaring a prefix, is the specific gap that would qualify. Rule generation by
loop is not such a capability.

## Consequences

- The supply-chain story simplifies to one sentence. There is no longer a
  "clean by default, tainted if you opt in" caveat to explain in `deny.toml`,
  `docs/release-readiness.md`, and `PROJECT_STATE.md`; `cargo audit` on the full
  `Cargo.lock` becomes clean rather than clean-with-four-allowed-warnings.
- Users who build from source with `--features starlark-policy` lose the feature.
  No such user is known: the crate is published nowhere, the feature ships in no
  release binary, and no user-facing document under `docs/` describes writing a
  `policy.star`. Their migration is mechanical — each `prefix_rule(...)` call
  becomes a `[[rules]]` entry with the same fields.
- A user who generated many rules programmatically has no in-tree replacement.
  They must generate the TOML themselves, outside Aegis. This is the real cost of
  the decision and it is accepted: generating configuration is a job for the
  user's own tooling, not for a scripting runtime embedded in a security tool.
- Decision 4 creates work beyond this ADR. `language-analysis` is currently in
  the same unverified position Starlark was, so accepting this ADR means
  accepting that a CI matrix job is owed for it.
- `tests/aegis_language_boundary.rs`'s `OTHER_WORKSPACE_CRATES` and
  `CONVENTION.md`'s crate inventory both shrink by one entry, and
  `CONVENTION.md:80-81`'s admission that `aegis-starlark` is unasserted becomes
  moot rather than outstanding.
- The `ROADMAP.md` history keeps its `aegis-starlark [DONE]` line and its
  `on_command` sketch as historical record. `ROADMAP.md` is the historical path,
  not a promise (per the document hierarchy fixed on
  [#199](https://github.com/IliasAlmerekov/aegis-shellguard/issues/199)), so it
  is annotated rather than rewritten.

## Alternatives considered

**Keep the feature and document the advisory exception more tightly.** Rejected:
it pays four advisories, a crate, and CI time for zero capability the tree does
not already have. Tightening the documentation of an exception does not change
what the exception buys.

**Keep the code in the tree but drop it from the `PRD.md` promise — the ADR-024
shape.** Rejected: ADR-024's shape exists to preserve a capability until its
re-entry conditions are met. Starlark has no capability to preserve, so the shape
would preserve maintenance cost only. The re-entry conditions are preserved in
this ADR instead of in the tree.

**Build `on_command(cmd)` so the DSL earns its keep.** Rejected as a 1.0 move: it
would put a scripting runtime on the decision path of a security tool for the
first time, on a dependency chain with four unmaintained crates, in the release
that is being narrowed rather than widened. It stays as the named re-entry
condition.

**Soften the `policy.star` startup error to a deprecation warning.** Rejected:
Aegis fails closed. A warning that scrolls past leaves the user believing a
policy is enforced when it is not, which is the precise failure mode the tool
exists to prevent.

## References

- [`PRD.md`](../../PRD.md) §5.2
- [`CONVENTION.md`](../../CONVENTION.md)
- [`deny.toml`](../../deny.toml)
- [`docs/release-readiness.md`](../release-readiness.md)
- [ADR-024](adr-024-language-aware-analysis-ships-opt-in-and-is-not-a-1-0-release-gate.md)
- [ADR-027](adr-027-one-1-0-release-gate-lives-in-the-issue-tracker.md)
- [#199 — Map: rebaseline Aegis 1.0 scope](https://github.com/IliasAlmerekov/aegis-shellguard/issues/199)
- [#202 — Which open findings block 1.0](https://github.com/IliasAlmerekov/aegis-shellguard/issues/202)
- [#203 — Inventory of documentation divergences](https://github.com/IliasAlmerekov/aegis-shellguard/issues/203)
- [#222 — Fate of the Starlark policy DSL before 1.0](https://github.com/IliasAlmerekov/aegis-shellguard/issues/222)
