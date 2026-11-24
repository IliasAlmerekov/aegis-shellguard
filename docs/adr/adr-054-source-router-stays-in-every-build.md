# ADR-054 — The source router stays in every build; `language-analysis` gates Tree-sitter and the worker

## Status

Accepted. Partially supersedes
[ADR-024](adr-024-language-aware-analysis-ships-opt-in-and-is-not-a-1-0-release-gate.md):
it moves the `language-analysis` feature cut line from the whole `src/analysis`
orchestration layer to the Tree-sitter runtime, grammars, and worker, keeps the
grammar-free source router in every build, and corrects ADR-024 §4 with triage
evidence. ADR-024 §2 and §3 remain in force unchanged.

## Context

[ADR-024](adr-024-language-aware-analysis-ships-opt-in-and-is-not-a-1-0-release-gate.md)
decided that language-aware analysis ships opt-in behind a `language-analysis`
cargo feature and is not a 1.0 release gate. ADR-024 §1 placed the feature cut
line around `aegis-language`, all of `src/analysis`, and the worker dispatch
branch. ADR-024 §4 stated that
[ADR-016](adr-016-effect-opaque-execution-uses-recovery-backstops.md)
effect-opaque recovery is complete without the stage and gets more conservative
(at worst an extra pre-exec snapshot) when the stage is absent.

That cut line did not account for what now lives in `src/analysis/router`. The
source router finds analysis targets in argv using `aegis-parser` alone: it
makes no Tree-sitter calls and links no grammars. Fixes for several published
security advisories and domain invariants live in the source router rather than
the scanner:

- **GHSA-c5pv** — later list segments (`;`, `&&`, `||`, `&`, newline)
- **GHSA-399c** — command runners (`uv run`, `uvx`, `pipx run`, `npx`,
  `poetry run`, `pipenv run`; [ADR-040](adr-040-explicit-runner-argv-is-visible-source.md))
- **GHSA-xj54** — executor option values (`git -c diff.external=...`, executor
  environment variables) and unclaimed launcher operands (`setsid ./script`,
  `setsid $HOME/pyx`)
- **GHSA-xprr** — ANSI-C quoting (`$'...'`) in unclaimed stages
- **[ADR-043](adr-043-dynamic-shell-words-require-approval-and-recovery.md)** —
  dynamic shell words, dynamic argv, and dynamic file-descriptor/write targets

During triage for issue #212, replacing the analysis call with the baseline
scanner assessment caused **40 of 57** regression shapes taken from those fixes
to drop from `prompt` to `auto_approve`, and most of those shapes also lost the
pre-exec snapshot. The scanner-level **GHSA-7564** control (git global options
before subcommands) did not change.

That probe establishes two facts:

1. Gating all of `src/analysis` under ADR-024 §1 would reopen published
   advisories in the default 1.0 build.
2. ADR-024 §4's claim that ADR-016 recovery becomes more conservative without
   the stage is false: runner, executor, launcher, and dynamic-argv shapes
   received no pre-exec snapshot at all when the source router was bypassed.

## Decision

### 1. The `language-analysis` feature gates Tree-sitter, the grammars, and the worker

The root `language-analysis` cargo feature (excluded from `default`) gates the
Tree-sitter runtime, the four language grammars, and the language worker
subprocess.

`aegis-language` stays a required dependency of the root crate and stays a leaf
crate (ADR-022 §4). A crate feature inside `aegis-language` gates `tree-sitter`
and the grammar crates, and the root `language-analysis` feature enables it.
Fuzz targets and crate benchmarks enable the crate feature directly.

The grammar-free source router is not gated and runs in every build. The
runtime entry point (`assess_with_language_analysis_async_in_cwd`) runs source
routing without starting the worker on the default build.

### 2. Default-build behavior for routed targets

On the default build (without `language-analysis`):

- The binary links no Tree-sitter runtime or grammar and never starts the
  language worker.
- The source router runs as normal, including the bounded `stat` and shebang
  check on launcher operands.
- Targets the router resolves (script file, inline body, runner child, executor
  value, or launcher operand with a verified shebang) are folded into
  `Assessment::effect_opaque`. They auto-approve only with a required pre-exec
  snapshot (ADR-016, ADR-031), matching how the scanner handles a direct
  `python3 file` today; if no snapshot can be planned, they follow the same
  required-recovery path. The default build does not record
  `DegradationReason::GrammarUnavailable`.
- Unresolved targets (`DegradationReason::DynamicSource` — dynamic command
  words, dynamic argv, `$` path operands, or ANSI-C quoted words) still record
  `AnalysisStatus::Degraded`, keeping the policy rule that degraded analysis
  requires explicit human approval.

### 3. Audit output on the default build

On the default build, an audit entry includes `analysis` only when the source
router records a degradation (`AnalysisStatus::Degraded`). It never contains
`AnalysisStatus::Complete` or a grammar `Match`. When the router resolves a
target, `analysis` is absent, and `effect_opaque` plus the snapshot plan carry
the decision.

With `--features language-analysis`, behavior and audit output are unchanged.

### 4. ADR-024 §2 and §3 stay in force; §4 is corrected

`aegis-types::analysis` and the `[language_analysis]` config section remain
unconditional (ADR-024 §2), and the opt-in post-1.0 release scope remains
unchanged (ADR-024 §3). ADR-024 §4's claim that ADR-016 recovery is complete
and strictly more conservative without `src/analysis` is superseded by this
decision: ADR-016 recovery for runner, executor, launcher, and dynamic-argv
shapes depends on the source router running in every build.

## Consequences

- Official 1.0 builds still link no Tree-sitter runtime or grammar and start no
  worker, preserving ADR-024's supply-chain and binary-size goals.
- Fixes for GHSA-c5pv, GHSA-399c, GHSA-xj54, GHSA-xprr, and ADR-043 remain
  active in the default build instead of regressing to `auto_approve` without
  snapshots.
- On the default build, scripts reached through runners, executors, or
  launchers are treated as `effect_opaque` (required snapshot) rather than
  parsed for semantic `Danger` matches; users who want script-body inspection
  enable `--features language-analysis`.
- A full-planner regression test pins both `DynamicSource` shapes (must require
  approval) and script-target shapes (never auto-approve without a required
  snapshot) across both feature configurations.
- Existing router false positives (#439, #446, #453, #454, #456, #459, #461)
  must be rechecked on the default build after the feature gate lands rather
  than assumed gone.
