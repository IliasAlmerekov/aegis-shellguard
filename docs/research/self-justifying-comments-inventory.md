# Self-justifying comments inventory

Ticket: #597 (map #591). Question: where do comments argue for the code instead of explaining it?

Scope: every tracked `.rs` file in `src/`, `crates/`, `tests/`, `benches/`, `fuzz/`, plus tracked shell scripts (`scripts/`, `plugins/`). `landing/` is excluded. Read-only; no code was changed.

## Method

Two scripted scans over `//`, `///`, `//!` and `#` comment lines, then manual reading of the worst hits.

1. Marker scan: "instead of", "rather than", "better", "we chose", "previously", "used to", "alternative", "considered", "this approach", "no longer", "originally", "formerly".
2. History and review-chatter scan: "previously", "used to", "no longer", "pre-fix", "before the fix", "was silently", and review references such as "#437 review, finding F1" or "review comment 4091038640".
3. Long-block scan: runs of 5+ consecutive comment lines (all kinds), and runs of 8+ plain `//` lines (inline comments, not doc comments).

Marker hits are an over-count. Many "rather than" and "instead of" sentences state a fail-closed invariant ("degrades rather than being treated as clean") and are worth keeping. Several "used to" and "no longer" hits are plain English in API docs ("used to restore the Snapshot", "no longer exists"). I judged these by reading samples, then applied a rough discount (about 40 percent of marker hits are real weeds).

Restatement-type weeds (a comment that repeats the next line of code) were only sampled. They are not in the counts below and are rare in this codebase; the dominant problem is argument and history, not restatement.

## Raw numbers

| Scope | Comment lines | Marker hits | History lines (true hits) | Review-chatter lines | Inline blocks of 8+ lines (lines) |
|---|---|---|---|---|---|
| `src/` | 4256 | 151 | ~20 | 38 | 25 (293) |
| `crates/aegis-parser` | 1643 | 40 | 5 | 14 | 4 (37) |
| `crates/aegis-scanner` | 1635 | 37 | 10 | 4 | 7 (65) |
| `crates/aegis-language` | 1793 | 26 | 2 | 1 | 11 (136) |
| `crates/aegis-config` | 1423 | 25 | 3 | 0 | 5 (49) |
| `crates/aegis-snapshot` | 818 | 23 | 6 | 0 | 1 (23) |
| `crates/aegis-sandbox` | 604 | 16 | 0 | 0 | 1 (8) |
| `crates/aegis-types` | 706 | 9 | 0 | 0 | 0 |
| `crates/aegis-audit` | 497 | 5 | 2 | 0 | 2 (19) |
| `crates/aegis-tui` | 141 | 2 | 0 | 0 | 1 (10) |
| `crates/aegis-policy` | 146 | 0 | 0 | 0 | 1 (8) |
| `crates/aegis-explanation` | 75 | 0 | 0 | 0 | 0 |
| `tests/` | 1899 | 74 | ~15 | 3 | 9 (83) |
| `benches/` | 104 | 5 | 0 | 0 | 4 (46) |
| `fuzz/` | 20 | 0 | 0 | 0 | 0 |
| shell scripts (`scripts/`, `plugins/`) | 281 | 11 | 0 | 0 | 0 |

Also: 488 comment lines cite an issue or PR number (`#NNN`). 151 of those are in `src/analysis`, 109 in `aegis-parser`, 62 in `aegis-scanner`. A bare issue or ADR link is fine under the rule. The weed is the sentence of argument that surrounds it. 17 lines cite a numeric review-comment id (for example `review comment 4091038640`), which is meaningless without GitHub access.

## Estimate: weed comments per crate

"Comment" means one comment block or sentence group, not one line. The estimate of weed lines is shown because the cleanup effort scales with lines.

| Crate or dir | Weed comments (est.) | Weed lines (est.) | Main kind |
|---|---|---|---|
| `src/` (mostly `src/analysis/router*`) | 110 to 140 | ~400 | multi-paragraph justification, review chatter, history |
| `aegis-parser` | 35 to 45 | ~130 | review chatter (`#437 review, finding F1`), "used to" history |
| `aegis-scanner` | 30 to 40 | ~90 | history in test comments (`previously`, `pre-fix`) |
| `aegis-language` | 25 to 30 | ~160 | long wire-format and module prose, "rather than" comparisons |
| `aegis-config` | 15 to 20 | ~55 | "rather than" design comparisons in ratchet docs |
| `aegis-snapshot` | 12 to 18 | ~50 | long test narrative, history |
| `aegis-sandbox` | 6 to 8 | ~25 | mostly keepable fail-closed notes |
| `aegis-audit` | 3 to 5 | ~15 | legacy-format history |
| `aegis-types` | 3 to 5 | ~15 | "rather than" design notes |
| `aegis-tui`, `aegis-policy`, `aegis-explanation` | 2 to 4 | ~15 | one 10-line and one 8-line block |
| `tests/` | 40 to 55 | ~130 | history ("used to", "pre-fix"), "can no longer weaken" copy |
| `benches/` | 8 to 10 | ~50 | long header essays defending bench settings |
| `fuzz/` | 0 | 0 | clean |
| shell scripts | 3 to 5 | ~15 | hook scripts are duplicated three times; rationale is short and mostly keepable |
| **Total** | **~290 to 380** | **~1150** | |

Confidence: the counts for marker-based kinds (comparison, history, review chatter) are within about 25 percent. The multi-paragraph kind is the least certain, because a long block can be a valid wire-format spec (keep) or an argument (delete). Restatements are not included.

## Worst offenders

Class legend: D = safe to delete, N = needs a decision, K = keep (with reason).

### `src/` (router and analysis)

| Location | What it is | Class |
|---|---|---|
| `src/analysis/router/segments/unclaimed.rs:322` | 37-line block; argues for the candidate-per-operand design, includes "stopping at the first one, as this used to, let ... shadow" history and a worked substitution-masking walkthrough | N (keep the two invariants: LauncherOperand routing and "only a shell-written `/` counts"; delete history and examples) |
| `src/analysis/router/segments/unclaimed.rs:418` | 23-line block of the same style | N |
| `src/analysis/router/segments/executor_config.rs:181` | 32-line doc comment; includes review comment id 4091038640 and a long collapse-by-`push_unique` argument | N (cut to 3 lines, drop the review id) |
| `src/analysis/router/segments/executor_config.rs:128`, `:350`, `:478` | 25, 21 and 18-line doc blocks, same pattern | N |
| `src/analysis/router/segments/dynamic_program.rs:20` | 28-line doc; "this function no longer makes that call itself", "instead of substring-matched", review comment 4091038665 | N (history lines at `:33`, review id at `:44` are D) |
| `src/analysis/router.rs:300-311` | `alias_scope_end` field doc argues segment granularity at length; ends in "deliberately the conservative direction" | N (the conservative-direction invariant is the keepable line) |
| `src/analysis/router.rs:273-276` | explains why `nodejs` is registered directly rather than as a basename family | D (the Debian note is the only needed line) |
| `src/analysis/router/segments/argv_walk.rs:13,24,42,45,164,174,185,208` | eight citations of numeric review-comment ids | D (ids are unreadable; keep the shape being described) |
| `src/analysis/router/segments/unclaimed.rs:276` | "first one found used to return immediately and hide the rest" | D |
| `src/analysis/router/segments.rs:202` | "each used to duplicate" history | D |
| `src/main.rs:341` | 24-line exit-code table | K (it is the exit-code contract; a table is not argument) |

### `aegis-parser`

| Location | What it is | Class |
|---|---|---|
| `crates/aegis-parser/src/env_launcher.rs:197,225,238,249` | four "#437 review, adversarial finding F1" citations plus a long story about a 512 KiB stack overflow | D for the review citations; K for the one-line "bound exists to prevent stack overflow" |
| `crates/aegis-parser/src/env_launcher.rs:6-30` | 25-line doc of `env_split_string_tokens`; lists every accepted shape | N (shape list is useful but belongs in tests) |
| `crates/aegis-parser/src/heredoc_data_consumer/message_flags.rs:58-72` | doc quotes two bugs that "used to resolve to" a wrong subcommand, cites "#396 review" | D for the history; K for "slot None means do not guess" |
| `crates/aegis-parser/src/lib.rs:93,147,162,218` | four "#437 review ... finding F1" citations | D |
| `crates/aegis-parser/src/tests/tokenizer_tests.rs:510,518` | "PR #463 review:" | D |
| `crates/aegis-parser/src/segmentation.rs:4` | 27-line header including "scan-oriented rather than execution-oriented" | N |

### `aegis-scanner`

| Location | What it is | Class |
|---|---|---|
| `crates/aegis-scanner/src/scanner/tests/issue_525.rs:344,528`, `issue_525_codex3.rs:4-5` | "Codex review of PR #529", "each case here is one way the walk used to be wrong" | D |
| `crates/aegis-scanner/src/scanner/tests/ghsa_7gcj.rs:7,11` | "was silently Safe", "previously Safe" | N (GHSA link is the keepable part; history sentence is D) |
| `crates/aegis-scanner/src/scanner/tests/basic.rs:509,625` | "previously had no keyword", "previously broke the aws s3 rules" | D |
| `crates/aegis-scanner/src/scanner/tests/construction.rs:80-81`, `edge_cases.rs:390`, `issue_344.rs:71`, `issue_357.rs:58` | "no longer", "pre-fix code", "exactly as before the fix" | D |
| `crates/aegis-scanner/src/scanner/prefix_rule.rs:259` | "better to miss an edge case than fire on `redis-cli GET FLUSHALL`" | K (states a precision trade-off the code cannot show; shorten to one line) |
| `crates/aegis-scanner/src/patterns/builtins_b.rs:341-344` | "keys on the recursive target rather than the mode" | K (explains the rule's key) |

### `aegis-language`

| Location | What it is | Class |
|---|---|---|
| `crates/aegis-language/src/operation.rs:163` | 30-line wire codec spec; first paragraph argues "hand-rolled rather than serde-derived" | N (field layout is a spec, K; the justification paragraph shrinks to one line with the ADR-022 link) |
| `crates/aegis-language/src/protocol.rs:1`, `worker.rs:1`, `lib.rs:1` | 19 to 34 line module headers | N (sample read: part spec, part rationale) |
| `crates/aegis-language/tests/{typescript,javascript,python}_corpus.rs:1`, `common/corpus_harness.rs:1` | 18 to 39 line headers that defend the corpus layout | D for the layout defense |
| `crates/aegis-language/src/worker.rs:190`, `protocol.rs:418` | "dropped rather than failing the whole experiment", "rather than duplicating the mapping" | N |
| `crates/aegis-language/benches/parse_latency_bench.rs:11,28` | "so the number is representative rather than degenerate" | D |

### `aegis-config`

| Location | What it is | Class |
|---|---|---|
| `crates/aegis-config/src/model/ratchet/direction.rs:145` | "wins, by deliberate decision rather than ..." | N (decision should live in an ADR link) |
| `crates/aegis-config/src/model/ratchet/{provenance,prune,language,sandbox}.rs` heads | "compile error rather than a silently skipped check" and similar | K (states a compile-time guarantee) |
| `crates/aegis-config/src/model/tests/ratchet/c3_residual.rs:1` | 16-line header: "Fix 1 / Fix 2 ... They must FAIL against the current (un-ratcheted) code" | D (history of a fix) |
| `crates/aegis-config/src/model.rs:25` | "every config layer before the fix (issue #319)" | D |
| `tests/config_integration.rs:180,257,261`, `snapshot_integration.rs:70`, `snapshot_ordering.rs:77`, `snapshot_rollback_live.rs:104`, `full_pipeline_snapshot.rs:49`, `support/mod.rs:139` | the sentence "the project layer can no longer weaken ... (C3)" copied across 8 files | N (replace each with `// C3 ratchet, ADR-013`) |

### `aegis-snapshot`, `aegis-sandbox`, `aegis-audit`, `aegis-types`

| Location | What it is | Class |
|---|---|---|
| `crates/aegis-snapshot/src/supabase/runtime/tests/hostile_overlay.rs:8-30` | 23-line test narrative: "used to be able to", "under the pre-fix whole-struct ratchet", "that old ratchet" | N (keep: layered loader is used so the ratchet is covered; delete history) |
| `crates/aegis-snapshot/src/git/mod.rs:253-257` | "Fail open rather than silently reporting 'not a repo'" | K (fail-open choice is a behavior invariant, tied to `SNAPSHOT_FAILED_CONTINUING`) |
| `crates/aegis-audit/src/logger.rs:353`, `tests/audit_snapshot_attempts.rs:18` | "logs written before `sandbox_status` existed", "before `snapshot_attempts` existed" | K (backward-compat reason code cannot show) |
| `crates/aegis-sandbox/src/landlock.rs:235,291,310`, `lib.rs:259` | "rather than what preparation predicted", "fail closed rather than run unconfined" | K (security invariants) |
| `crates/aegis-sandbox/src/macos.rs:556,620` | "spawn instead of exec() so the test process is not replaced" | K (one line; says why) |
| `crates/aegis-types/src/analysis.rs:198-201`, `policy.rs:6` | "rather than assigning RiskLevel", "rather than in either crate" | K (layering rule) |

### `tests/`, `benches/`, `scripts`

| Location | What it is | Class |
|---|---|---|
| `tests/docker_integration.rs:1` | 32-line `//!` "Why these exist" essay | N |
| `tests/full_pipeline_policy.rs:435` | "Previously (before non-interactive mode), empty stdin was treated as" | D |
| `tests/analysis_orchestrate_runtime.rs:555`, `tests/analysis_orchestrate.rs:100,595` | "pre-fix implementation", "routing used to look only at", 15-line block | D |
| `tests/contracts_docs.rs:199,279` | "allowed to describe the old behaviour" | K (it documents a test rule about docs) |
| `benches/router_bench.rs:13` | 18-line header defends bench group config and the plan() choice | N (cut to the "no worker spawn" invariant, ADR-022 §0) |
| `benches/startup_bench.rs:23,38,58,107` | "rather than repeating the production call", "instead of ..." | D |
| `scripts/test-tamper-check.sh:1-29` | 29-line header; lists what it flags | K (it is usage doc for a script) |
| `scripts/lint.sh:27` | "Stop rather than install the toolchain or fall back" | K (states the rule) |
| `scripts/hooks/*.sh`, `plugins/aegis/hooks/*.sh` | "fail closed rather than let the command run" repeated across 3 to 5 copies | K (invariant), but the duplicated copies are a separate issue |

## Patterns to fix first

1. **Review chatter** (about 60 lines, `src/analysis/router/segments/*`, `aegis-parser`, scanner tests): citations like `#437 review, finding F1` and numeric review-comment ids. Safe to delete, no decision needed.
2. **History in tests** (about 70 lines): "used to", "previously", "pre-fix", "no longer". Safe to delete. A regression test comment should say what must hold and link the issue or GHSA.
3. **Repeated boilerplate**: the C3 "project layer can no longer weaken" sentence appears in 8 test files; one-line ADR reference replaces it.
4. **Multi-paragraph argument in the router** (`src/analysis/router/segments/unclaimed.rs`, `executor_config.rs`, `dynamic_program.rs`, `router.rs`): the largest and most decision-heavy cluster, about 10 blocks and 300 lines. Needs a decision per block because they hold some real invariants buried in prose.
5. **Header essays** in benches and test files: 18 to 39 lines; need a decision on how much is spec versus defense.

## Before and after examples

### 1. Review chatter and history (`crates/aegis-parser/src/env_launcher.rs:238`)

Before:

```
// One level deeper than the bound: resolution must record the stop
// rather than silently resolve to nothing (#437 review, finding
// F1) — callers that would otherwise treat an empty/partial result
// as an ordinary non-program stage need this to fail closed.
```

After:

```
// Past the nesting bound, resolution records the stop so callers fail closed.
```

### 2. Multi-paragraph justification (`src/analysis/router/segments/unclaimed.rs:322`)

Before: 37 lines covering routing choice, why the first candidate is no longer returned ("as this used to"), three worked command examples, and the masking algorithm.

After:

```
// Routed as `LauncherOperand`, not `DirectExec`: the net picked these operands
// out of an unclaimed stage, so a missing path or directory resolves
// speculatively instead of degrading (#384, #430).
// Every distinct path-like operand is a candidate; a flag value can shadow the
// real script. Only a `/` the shell writes itself counts, so substitutions are
// masked first.
```

Worked examples move to the tests in `src/analysis/router/tests/unclaimed_launcher_operands.rs`, which already carry them.

### 3. Repeated copy and rejected-alternative framing (`tests/config_integration.rs:180`)

Before:

```
// The project layer can no longer weaken `allowlist_override_level` below
// the global value (C3 ratchet), so the trusted global config opts ...
```

After:

```
// C3 ratchet (ADR-013): the project layer cannot weaken this field.
```

## What I did not check

- Restatement comments were sampled, not counted.
- Per-line classification of all 424 marker hits was not done; the offender list is the worst clusters, not every line.
- `fuzz/` and `aegis-explanation` showed nothing worth listing.
- No security bypass was found while reading.
