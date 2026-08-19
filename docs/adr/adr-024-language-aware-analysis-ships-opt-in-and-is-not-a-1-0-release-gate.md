# ADR-024 — Language-aware analysis ships opt-in and is not a 1.0 release gate

## Status

Accepted

## Context

[ADR-022](adr-022-language-aware-analysis-is-an-additive-isolated-stage.md) fixed
the architecture of language-aware analysis: an additive slow path behind an
ephemeral, isolated Tree-sitter worker. That architecture is implemented. The
`aegis-language` crate carries the pinned runtime, the grammar manifest, and four
adapters (Python, JavaScript, TypeScript, Shell/Bash); the parent-side
orchestration lives in `src/analysis`; the worker is reached through an internal
flag on the shipping binary. The stage is wired end to end, runs by default, and
has no runtime switch.

What is *not* settled is the release meaning of that code. `ROADMAP.md` calls it
a Pre-1.0 milestone with four adapters "production-qualified and default-on".
`docs/release-readiness.md` carries eight unchecked items inside the Minimum
Launch Checklist — the list of launch blockers. `PRD.md`, approved 2026-06-15,
does not mention the stage at all in its scope sections. Three documents, three
different answers to "does 1.0 ship this?".

Two facts decide the question.

First, where 1.0 differentiation actually comes from. Aegis ships 70+ typed
built-in rules. The comparable built-in detection in Codex is a single Unix rule
for forced `rm`; its `execpolicy` engine ships no default rule corpus at all, and
rules accumulate from the user's own "always allow" decisions. That is a
two-order-of-magnitude gap in the cheapest part of the product. The value a 1.0
user gets is the rule corpus, the Snapshot and recovery contract, and the
confinement derived from them — not the semantic stage. The honest counterweight:
Codex parses the body of `sh -c` for literal commands but does not read script
*files*, and reading source is exactly what this stage adds. That is a real
capability gap in the other direction, and it is why the stage is deferred rather
than abandoned.

Second, what shipping it unqualified costs. The four grammars are native C build
inputs — the sole sanctioned exception to the project's no-C-dependencies rule.
Compiled into the 1.0 artifact they sit permanently inside the `cargo deny` /
`cargo audit` supply-chain surface and inside the binary-size budget, whether or
not the qualification matrix that justifies them has been completed. Paying the
supply-chain and size cost of a stage while withdrawing the promise that the
stage works is the worst available combination.

The remaining question is mechanical: the analysis vocabulary is not confined to
`aegis-language`. `aegis-types::analysis` defines `AnalysisSummary`,
`AnalysisStatus`, `TargetAnalysis`, and the typed degradation model with no
Tree-sitter dependency, and the audit entry already carries
`analysis: Option<AnalysisSummary>` — absent on v1 lines and on entries with no
language analysis. The config model likewise carries `[language_analysis]`
budgets that participate in the project-layer security ratchet. Those are release
contracts; the grammars are a build input. The two must be cut apart.

## Decision

### 1. The stage is built behind an opt-in cargo feature, off in official 1.0 binaries

Language-aware analysis is gated by a `language-analysis` cargo feature that is
not in `default`, mirroring the existing `starlark-policy` precedent. The feature
gates the `aegis-language` dependency, the `src/analysis` orchestration modules,
and the internal worker-dispatch branch on the binary entry point. Official 1.0
release artifacts are built without it, so no Tree-sitter runtime and no grammar
is linked into them.

Users who want the stage build it themselves and accept its unqualified status.

### 2. The analysis vocabulary and config surface stay unconditional

`aegis-types::analysis` and the `[language_analysis]` config section are compiled
unconditionally, independent of the feature.

The audit log format is part of the public contract from v1. A build-time feature
must not produce a second audit shape, and a build without the feature already
emits a valid line: `analysis` is simply absent. The same argument covers the
generated config schema, which is one artifact per release rather than one per
feature combination, and the ratchet fields, which govern how an existing project
`.aegis.toml` is parsed on upgrade. A dead field in the config model is cheaper
than a contract that varies by build.

Note the distinction this preserves: an absent `AnalysisSummary` means the stage
did not run or was not built. `AnalysisStatus::NotApplicable` means the stage ran
and the target had no analyzable source. They are different facts.

### 3. This is a scope decision, and early qualification does not reverse it

The stage is deferred because it is not what 1.0 sells, not because it is running
late. Completing the qualification matrix before the 1.0 release does not return
the stage to 1.0 or to `default` features.

The eight items currently inside the Minimum Launch Checklist are preserved
verbatim as the criteria for turning the feature on by default in a later 1.x
release. They stop being launch blockers and become the re-entry conditions. The
target release is "after 1.0" with no version number: naming 1.1 would create a
new dated promise of exactly the kind this decision withdraws, before the cost of
the remaining qualification is known.

### 4. ADR-022 is unchanged, and ADR-016 is unaffected

ADR-022 stays `Accepted` with no status edit. It describes the architecture of
the stage, and that architecture is not what changed — the code remains, compiles
under the feature, and keeps its tests. Recording a release decision by mutating
an architecture ADR's status would misstate the architecture.

The effect-opaque recovery backstop of
[ADR-016](adr-016-effect-opaque-execution-uses-recovery-backstops.md) is complete
without this stage. Its v1 detection is argv-shape based — an interpreter with a
script-file-looking token, `sh -s`, pipe-to-shell — and it explicitly performs no
hot-path `stat()`. ADR-022 already states that successful Script source
inspection never makes Script-file execution trusted, so the stage could never
have relaxed the backstop. Without the stage, ADR-016 is more conservative, not
weaker: the error direction is an extra pre-exec snapshot.

## Consequences

- 1.0 release artifacts contain no native C build input. The `cargo deny` /
  `cargo audit` supply-chain gate and the binary-size budget for 1.0 are measured
  on a build without grammars, so the 9.5 MiB figure recorded in
  `docs/performance-baseline.md` describes a feature-on build and is not the 1.0
  size.
- CI keeps exercising the stage with an explicit `--features language-analysis`
  rather than freezing it. The fuzz targets and the crate benchmarks depend on
  `aegis-language` directly and are unaffected by the root feature. The cost is
  CI time; the benefit is that the corpora, benchmarks, and evidence stay live
  for the later default-on decision instead of being rebuilt from nothing.
- `tests/l1_qualification_ci.rs` inverts. It currently asserts that every release
  target builds the shipping binary so that every qualified grammar is statically
  present in every official binary. Under this decision the invariant to pin is
  the opposite: the default build links no Tree-sitter.
- The silence of `PRD.md` on the stage becomes correct rather than a divergence
  to repair.
- `ROADMAP.md` and `docs/release-readiness.md` contradict this ADR until they are
  aligned; both still assert that official binaries must contain the pinned
  grammar set. Until that alignment lands, this ADR is the authoritative record.
- The stage keeps running in developer builds that enable the feature, so the gap
  it closes — reading script *files* — is deferred for 1.0 users, not discarded.
