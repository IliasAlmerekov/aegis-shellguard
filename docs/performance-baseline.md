# Performance baseline policy

This document defines the repeatable benchmark strategy for Aegis scanner
performance checks.

## The two budgets

Aegis publishes two performance budgets, and confusing them is easy enough that
it already happened in this repository. `ADR-034` defines them and records the
measurement they came from; `PRD.md` §6 carries the numbers.

- **Assessment budget** — what one `assess` call may cost on any input,
  including the worst one. 2 ms. A typical safe command costs about 2.6 µs; the
  worst inline-script body under `MAX_INLINE_SCRIPT_LEN` costs about 1.9 ms, so
  the budget binds at the worst case and nowhere near the typical one.
- **Startup cost** — what one process invocation costs from process start to
  the decision, before exec. Measured at 22 ms, with a recorded ceiling of
  30 ms. Roughly 20 ms of it is construction: the `Scanner`, the runtime
  context, the async runtime. Classification inside it is 0.02 ms.

Two rules follow from this, and both matter when reading any number below.

A batch figure is not a per-command figure. A row that times N commands in one
Criterion iteration reports the cost of the batch, and comparing that directly
against a per-command budget is off by a factor of N. The removed row that
timed 1,000 safe commands per iteration was compared that way in project
documents.

Every budget here is a **mean**, not a percentile. Criterion writes `mean`,
`median`, `std_dev` and `MAD` into `estimates.json` and no percentiles at all,
and `aegis_benchcheck` reads the mean. Earlier wording in `PRD.md` said "p99",
which no tool in this repository could check.

## What is checked

The CI performance job runs:

```bash
cargo bench --bench scanner_bench
cargo bench --bench no_source_bench -p aegis-language
cargo bench --bench parse_latency_bench -p aegis-language
cargo bench --bench startup_bench
cargo run --bin aegis_benchcheck -- --baseline perf/scanner_bench_baseline.toml --criterion-root target/criterion
```

All four `cargo bench` invocations write into the shared workspace
`target/criterion` root, so a single `aegis_benchcheck` run evaluates every
policy row. A policy row whose Criterion result is missing fails the job, so
dropping a bench invocation from CI cannot silently drop its gate.

That guarantee is not free: Criterion leaves
`target/criterion/<name>/new/estimates.json` behind from earlier runs, so a bench
that stops executing yields a stale **PASS** rather than a failure. The CI job
caches `target/` under a `perf-` key with `restore-keys`, so it is exposed to
this too — which is why the job discards `target/criterion` before the first
`cargo bench`. Do the same locally (`rm -rf target/criterion`) before trusting a
green `aegis_benchcheck`.

Renaming a Criterion group or `BenchmarkId` is the one drift this does not catch
by itself: rename both the bench and its policy row and the gate stays green
while measuring something else. `ci_keeps_safe_and_slow_path_qualification_benches_on_the_performance_gate`
in `tests/l1_qualification_contracts.rs` is the guard for that — it pins the
`run:` lines and the policy row names at PR time.

Criterion produces per-benchmark `estimates.json` files under `target/criterion/`.
`aegis_benchcheck` reads the checked-in policy file and compares each benchmark's
observed **mean** against the corresponding baseline budget.

## Checked-in baseline

The machine-readable policy lives at:

- `perf/scanner_bench_baseline.toml`

It covers three surfaces.

### Scanner hot path (`benches/scanner_bench.rs`)

- `safe_command_assess`
- `100_dangerous_commands`
- `heredoc_worst_case`

The initial values were rounded from a local benchmark capture on **2026-04-11**
and then given extra headroom so the policy is stable on shared CI runners.
`heredoc_worst_case` was rebaselined on **2026-08-04** — see below.

#### `heredoc_worst_case` rebaseline (2026-08-04)

`baseline_ns` moved from `300_000` to `1_000_000`. This row is the one place in
the file where a rebaseline followed a **real** four-fold slowdown rather than a
bench change, so the evidence is recorded here instead of only in a PR
description.

What the first enforcing CI run reported (the `tee`/`pipefail` fix in `6f886b3`
is what made the gate enforce at all):

```text
FAIL heredoc_worst_case observed 879.966 µs baseline 300.000 µs delta +193.3% threshold +30.0%
```

Bisect over `d12e971..8efd524` with a 400 µs cut (`cargo bench --bench
scanner_bench -- --quick heredoc_worst_case`, local release build):

| Revision | Mean |
| --- | ---: |
| `d12e971` — baseline capture point | 175 µs |
| `0d750ec` | 166 µs |
| `bdfbaf9` — `fix: normalize launcher prefix detection` (first over the cut) | 465 µs |
| `8efd524` — last commit before the language-aware series | 698 µs |
| `6f886b3` — current | 678 µs |

Three conclusions the numbers support:

1. **The old value was honest.** 175 µs at `d12e971` is consistent with the
   `300_000` budget; the baseline was not padded to hide anything.
2. **Language-aware analysis is not involved.** `Scanner::assess` returns
   `analysis: None` and never enters `aegis-language`; the slowdown is fully
   present at `8efd524`, before the series began. The four `parse/*` rows and
   `no_source_does_not_start_worker` all pass well under their ceilings.
3. **The cost is accumulated detection work, not one defect.** The ramp is
   gradual across the shell-security commits, with the largest single step at
   `bdfbaf9` (ADR-014), which made the inline-script body a *second* regex scan target
   (`effective_token_slices` → `join(" ")` → `full_scan`, on top of the
   `recursive::scan_targets` pass). The redundant second scan of the same body is
   tracked as **P3-9** in `TASKS.md`; closing it should recover roughly half of
   this row and is the reason the new ceiling is 1.3 ms rather than a looser one.

The growth is linear and bounded by the existing input caps, measured on the
same command shape at increasing body sizes:

| Inline body | Command bytes | Mean | Cost per KB |
| --- | ---: | ---: | ---: |
| 50 lines | 1,459 | 163 µs | 114 µs |
| 200 lines (the bench) | 6,309 | 734 µs | 119 µs |
| 400 lines | 13,109 | 1,500 µs | 117 µs |
| 800 lines | 26,709 | 250 µs | 10 µs |
| 1,600 lines | 56,909 | 528 µs | 10 µs |

The collapse past 26 KB is `MAX_INLINE_SCRIPT_LEN` (16 KiB) returning `SCAN-002`
before the scan, so the worst case is a body just under that cap: about 1.9 ms
locally. That is inside the 2 ms `Assessment budget` but with little margin on a
slower runner — a second reason P3-9 matters. This row is the one that makes the
budget bind: the budget covers any input, and this is the worst input the caps
allow.

#### Batch safe-command row removed (2026-09-15)

The row beside it used to measure 1,000 cycled safe commands per iteration,
last recorded at 2.602 ms — 2.6 µs per command. That figure was read as a
per-command number and compared against the 2 ms `Assessment budget` more than
once in project documents, which it never was: a batch mean cannot be compared
against a per-command budget (ADR-034). The row is removed rather than fixed in
place, and `safe_command_assess` replaces it with one `assess()` call per
iteration, timed directly against the budget it was always meant to check.

#### `safe_command_assess` and `scanner_construction` rebaseline (2026-09-16)

Both rows shipped with `baseline_ns` set within 5% of a single local capture,
which left no room for run-to-run variance: three repeated local runs of
`safe_command_assess` spread from 568.7 ns to 725 ns — a 27.5% swing against a
25% threshold — and `scanner_construction` spread from 7.05 ms to 8.34 ms
against a baseline that put the gate at 9.5 ms. Both would flap on a runner no
slower than the one that captured them. The two rows now carry roughly the
same ~2x margin over the observed maximum that the Iteration 10 slow-path
ceilings already use (see the table under "Language-aware slow path" below):
`safe_command_assess` moved from `700` to `1_500`, `scanner_construction` from
`7_600_000` to `16_500_000`. `runtime_context_construction` was rebaselined
the same way from repeated local runs (max observed 11.045 ms) to
`22_000_000`, alongside the description fix below.

#### Discarded scanner builds removed; lazy compile and keyword narrowing tried and reverted (issue #319, 2026-09-16)

One real problem, fixed, and two tried-and-rejected ideas:

**Fixed: `validate_custom_patterns` no longer builds a scanner it throws away.**
Both `AegisConfig::validate_runtime_requirements` (called once by every
`RuntimeContext::new_with_policy_path`) and `AegisConfig::load_for_internal`
(called once per config layer found on disk) used to call it unconditionally,
and it built a full `Scanner` — merging in the built-ins and compiling every
regex — purely to discover that there was nothing to validate whenever the
custom-pattern slice was empty, which is the common case. It now returns
`Ok(())` immediately on an empty slice. `load_for_internal` also now skips the
per-layer scanner rebuild for a layer that added no new custom patterns (the
cumulative set was already checked by whichever layer did add them); the
layer's own general validation — audit, allowlist, blocklist — still always
runs, so a misconfiguration introduced by a patternless layer is still caught
and attributed to that layer's file.

This is what `runtime_context_construction` mostly paid for beyond its
`id -un` spawn: repeated local captures after the fix land at 873 µs–1.13 ms,
down from a baseline of 22 ms. Rebaselined to `2_200_000` (~2x the observed
maximum, same margin methodology as the 2026-09-16 rebaseline above).
`scanner_construction`'s baseline is unchanged (`16_500_000`) — see below for
why.

**Tried and reverted: lazy built-in regex compilation.** The issue's premise
was that `Scanner::try_new` compiles every built-in pattern's regex eagerly,
whether or not the process ever sees a command that needs it, and that a
`$SHELL` proxy rebuilding the scanner once per invocation pays that cost on
every command regardless of outcome. Deferring built-in compilation to the
first time `full_scan` reached a given pattern (custom patterns stayed
eager, since a malformed one must remain a typed construction-time error)
was implemented, then reverted before merging: a one-off local measurement
of realistic cold `assess()` calls (fresh `Scanner` per call, matching a
`$SHELL` proxy's fresh-process-per-command shape — no warm cache carries
over between commands) showed built-in-compile-on-demand pushing ordinary
inputs past the 2 ms Assessment budget:

| Input | Cold `assess()` cost (fresh scanner, release) |
| --- | ---: |
| Single command, e.g. `rm -rf /home/user/old-project` | mean 0.5–0.8 ms, p95 up to ~1.6 ms, occasional noise spikes to 4–5 ms |
| 8-clause `;`-chained compound command (a plausible cleanup/deploy script) | mean 2.22 ms, p95 3.68 ms |
| 41-clause `;`-chained command spanning every built-in category (adversarial but syntactically ordinary) | mean 5.0 ms, max 10.2 ms, 29 of 41 patterns compiled in one call |

A compound command touching several pattern categories at once is ordinary
shell syntax, not a contrived edge case, and ADR-034's Assessment budget
covers "one `assess()` call on any input." Regex compilation therefore stays
part of the (30 ms-budgeted) Startup cost, where it already had comfortable
headroom, rather than moving into the 2 ms Assessment budget. Net effect:
`scanner_construction`'s cost is unchanged by this issue (still dominated by
eager regex compilation), which is why its baseline stays at `16_500_000`.

**Also tried and reverted: narrowing `full_scan` to keyword-matched patterns.**
Alongside the lazy-compile attempt, `full_scan` was changed to run a second,
overlapping Aho-Corasick pass over the command and only evaluate the regexes
whose extracted keyword actually showed up, instead of evaluating every
pattern in the applicable `universal`/`by_program` bucket unconditionally.
It was committed on the #319 branch, and it looked safe at first: fewer regex
evaluations per `full_scan`, and no test failed. The commit and its revert
were squashed before merging, so neither is in `main`.

It shipped a false negative. `extract_keywords`'s `find_embedded_literal`
walks `EXEC-006`'s `sh` alternative
(`^sh\s+(?:--[a-z-]+\s+)*-[a-zA-Z]*c\b`), discards the two-character literal
`"sh"` against its three-character floor, then keeps scanning *through the
regex syntax* of the optional group and accumulates `":"`, `"-"`, `"-"` as
the pattern's "keyword" — text no command matching `EXEC-006` is required to
contain. Reproducer: on the commit with the narrowing, both
`Scanner::assess("sh -c id")` and `full_scan("sh -c id", Some("sh"))` report
zero matches; on `main`, the same calls report `Warn` with `EXEC-006`.

The extractor's flaw predates this change, and it is not harmless in
`quick_scan` either. `quick_scan` returns `false` when no keyword matches, so
a keyword the command is not required to contain can turn a matching command
into `Safe`. That is the false negative `mod.rs:38` forbids. Today
`quick_scan` still passes `bash`, `sh`, `dash`, `zsh`, `ksh`, and `fish` with
`-c` (checked 2026-09-17), but only because another keyword in the automaton
matches those commands, not because `":--"` does. The extractor fix is
tracked separately. The narrowing made the same flaw skip the regex itself,
which is why it was removed.

The narrowing was removed rather than repaired: fixing `find_embedded_literal`
and proving the keyword-to-pattern mapping sound is real work belonging to
its own issue, not a condition on finishing #319, whose ask was construction
cost on the safe path, not `full_scan`'s regex count on the flagged path.
`full_scan` now evaluates every pattern in the applicable bucket
unconditionally again. `scanner_construction` is unaffected either way — the
narrowing changed `full_scan`, not `try_new` — so its baseline is unchanged
at `16_500_000`, and `runtime_context_construction` stays at `2_200_000`,
since that rebaseline came from the `validate_custom_patterns` fix above, not
from the narrowing.

**One-off split (issue's ask): how much of `Scanner::try_new` is Aho-Corasick
construction vs. regex compilation, and how much of the latter is Unicode
case-folding.** Not perf-gate rows — they would freeze `try_new`'s internals.

| Component | Time (2,000 iterations, release) |
| --- | ---: |
| Aho-Corasick construction + keyword-index bookkeeping | ~490 µs |
| Compiling all 41 built-in regexes, case-insensitive (current behavior) | ~4.79 ms |
| ...of which: same regexes compiled case-*sensitive* (no case folding at all) | ~2.49 ms |
| ...of which: Unicode case-folding overhead alone (case-insensitive minus case-sensitive) | ~2.16 ms (~46% of compile time) |

Regex compilation is ~90% of `scanner_construction`'s cost, and Unicode case
folding is close to half of *that* — a real lever, investigated as a follow-up
to the reverted lazy-compile attempt: if `RegexBuilder::unicode(false)`
(ASCII-only case folding, matching what the Aho-Corasick gate already does)
could replace `case_insensitive(true)` for built-ins, eager compilation would
get cheap enough that the lazy-compile Assessment-budget problem above might
not need lazy compilation to begin with. It was not adopted: `unicode(false)`
also turns `.`, `\s`, `\d`, and `\w` into byte-oriented, ASCII-only matchers,
and 13 of the 41 built-in patterns — every one built around `.`/`.+` for a
pipeline body (`PKG-001`, `PKG-002`, `PKG-004`, `EXEC-001`, the fork-bomb
pattern `PS-004`, and others) — fail to *compile* under it at all ("pattern
can match invalid UTF-8"), because byte-oriented `.` could split a multi-byte
UTF-8 sequence. Making it safe would mean rewriting close to a third of the
built-in pattern bodies with explicit byte-safe constructs (e.g. scoping
`(?-u)` to just the literal keyword and re-verifying every affected pattern
against non-ASCII input), which is a correctness-sensitive change to the
pattern definitions themselves, not a construction-cost change — out of scope
for this issue. Recorded here as "cannot be removed without a separate,
riskier change; here is why," per the issue's own accepted outcome for this
measurement.

Same ~2x-margin methodology as the 2026-09-16 `safe_command_assess` rebaseline
above: these are construction-heavy rows with real run-to-run spread, not a
value pinned within a few percent of one capture.

### Startup cost (`benches/startup_bench.rs`)

Four rows split the `Startup cost` from the `Assessment budget` (ADR-034):
one process invocation is dominated by construction, not by classification, so
a gate on `assess()` alone never saw most of what an agent actually pays. The
first three are original to ADR-034; the fourth,
`runtime_context_custom_pattern_construction`, is documented separately below
(issue #397).

- `scanner_construction` — `PatternSet::load()` plus `Scanner::try_new()`,
  timed on every iteration since the process-wide `BUILTIN_SCANNER` static
  cannot be re-initialised in a loop. Models the one-per-process cost of
  building the scanner, which is exactly what a `$SHELL` proxy pays once per
  command.
- `runtime_context_construction` — `RuntimeContext::new()` from a default
  config. A default config has no `custom_patterns`, so this path reuses the
  already-warm `BUILTIN_SCANNER` static instead of building a scanner of its
  own — this row and `scanner_construction` measure disjoint work and can be
  added together. Config discovery from disk is deliberately excluded here;
  only `startup_safe_command` below covers it. This row also spawns and waits
  on an `id -un` child process — `detect_effective_user()` in
  `src/runtime/user.rs` walks `PATH` for an `id` binary and shells out to it —
  so part of what looks like in-process construction cost is actually process
  spawn, measured locally at roughly 1 ms of the row's several-millisecond
  total.
- `startup_safe_command` — one full `aegis -c "ls -la" --output json` process
  invocation, `HOME` pointed at a fresh `TempDir` so no repository
  `.aegis.toml` is discovered and the number does not depend on where `cargo
  bench` was launched, `AEGIS_CI=0` forced so the non-CI branch is what gets
  measured, evaluation mode only (the path to the decision, no exec, no audit
  write). This is the only row that includes config discovery from disk.

#### `runtime_context_custom_pattern_construction` baseline (issue #397, 2026-09-22)

A fourth `Startup cost` row: `RuntimeContext::new()` from a config carrying
one `custom_patterns` entry, the branch `runtime_context_construction` does
not exercise. A non-empty custom-pattern slice sends `aegis_scanner::scanner_for`
down its other branch, which builds a fresh `Scanner` merging the 41 built-ins
with the custom set (`crates/aegis-scanner/src/lib.rs:47-53`,
`crates/aegis-scanner/src/patterns.rs:132-151`) instead of cloning the
already-warm `BUILTIN_SCANNER` static, so its cost lands close to
`scanner_construction`'s range rather than `runtime_context_construction`'s.

`baseline_ns` was first set from three local release captures (`cargo bench
--bench startup_bench -- --quick runtime_context_custom_pattern_construction`),
which spread 9.266–10.492 ms — the same ~2x-margin-over-observed-max
methodology as the 2026-09-16 `safe_command_assess`/`scanner_construction`
rebaseline above: `21_000_000`, roughly double the observed maximum.

Corrected 2026-09-22 from PR #402's `performance` CI job: observed 9.743 ms
(`PASS runtime_context_custom_pattern_construction observed 9.743 ms baseline
21.000 ms delta -53.6% threshold +25.0%`), comfortably inside the local-capture
baseline above. Re-baselined to `19_500_000` — roughly double the CI-observed
value, the same margin-over-a-single-capture discipline the 2026-09-16
rebaseline above adopted after `safe_command_assess`/`scanner_construction`
flapped on baselines pinned within 5% of one capture. This row keeps the
default `+25%` threshold rather than `startup_safe_command`'s widened `+50%`,
since it times in-process construction rather than a whole process
invocation and does not carry that row's process-spawn variance.

### Language-aware slow path, since Iteration 10 (the two `aegis-language` benches)

- `no_source_does_not_start_worker` (`benches/no_source_bench.rs`)
- `parse_latency_per_grammar/parse/{python,javascript,typescript,bash}`
  (`benches/parse_latency_bench.rs`)

These are padded **ceilings**, not measured means. Each was rounded up from a
local benchmark capture on **2026-08-03** to roughly 2x the observed value, so
the effective gate is that ceiling plus the +25% threshold:

| Row | Captured mean | `baseline_ns` | Effective ceiling |
| --- | ---: | ---: | ---: |
| `no_source_does_not_start_worker` | 950 ns | 2,000 | 2,500 ns (2.6x) |
| `parse/python` | 25.8 µs | 50,000 | 62.5 µs (2.4x) |
| `parse/javascript` | 18.5 µs | 30,000 | 37.5 µs (2.0x) |
| `parse/typescript` | 21.6 µs | 35,000 | 43.8 µs (2.0x) |
| `parse/bash` | 13.4 µs | 25,000 | 31.3 µs (2.3x) |

The first ratchet is deliberately loose: it establishes a ceiling that catches an
order-of-magnitude slow-path regression without failing on developer-machine or
runner variance, and can be tightened once CI-side variance is known.
`no_source_does_not_start_worker` gets the widest relative headroom because it is
the smallest absolute measurement in the file (sub-microsecond, ~95 ns per
command) and therefore the most sensitive to host differences.

### Iteration 10 production qualification evidence

This is the local measurement record for the **four foundation adapters under
qualification**. It is
evidence for the Iteration 10 gate, not a release-enable claim: the required CI
contexts and all-four-target artifacts remain the authoritative release record.
The measurements below were taken on 2026-08-04 from this checkout's release
build; host-specific figures are observations, while the checked-in Criterion
ceilings above are the repeatable regression policy.

| Surface | Evidence | Result | Qualification interpretation |
| --- | --- | --- | --- |
| No-source safe path | `cargo bench --bench no_source_bench -p aegis-language` | 1.01 µs for the ten-command corpus (about 101 ns/command) | Worker-free and below the 2.5 µs policy ceiling. |
| Per-grammar parse | `cargo bench --bench parse_latency_bench -p aegis-language` | Python 28.7 µs; JavaScript 21.8 µs; TypeScript 24.5 µs; Bash 14.6 µs | Every row is below its checked-in Criterion ceiling. |
| Worker cold-session latency | `/usr/bin/time` around one framed `--internal-language-worker` Python parse | below the tool's 10 ms display resolution | This is process start + one bounded request on the release binary; use the 100 ms total deadline, not this host measurement, as the enforced bound. |
| Worker warm-session latency | Not applicable to the production orchestration | no reusable session | `orchestrate` deliberately spawns, closes, and reaps one ephemeral worker per queued target. The protocol can carry a bounded sequence, but production does not reuse a warm worker; a future reuse optimization needs its own benchmark and review. |
| Peak worker RSS | five cold worker samples with `/usr/bin/time` | 4.1–4.3 MiB | The direct worker process stayed within this observed host range; it is evidence only, not a cross-platform memory promise. |
| Aggregate-timeout boundary | `tests/analysis_orchestrate_runtime.rs::run_records_target_aggregate_and_total_time_budget_exhaustion` | enforced at the configured total deadline | The default is 100 ms. The regression asserts typed `LimitExceeded` while retaining prior target results; no timer-derived throughput claim is made. |
| Per-target release-binary size | native `target/release/aegis` | 9.7 MiB | Local native size is recorded for drift detection. Grew from 9.5 MiB when the embedded bubblewrap fallback (ADR-029 §3) was added; the embedded `bwrap` is ~116 KiB. Exact sizes for Linux musl x86_64/aarch64 and macOS x86_64/aarch64 must come from the required CI contexts; local cross-target sizes are not substituted for release artifacts. |

The worker cold-session and RSS commands send one length-bounded `Parse`
request directly over the documented pipe protocol and discard only the binary
response. They neither read a script file nor execute analyzed source. The
warm-session row is intentionally explicit rather than fabricated: an ephemeral
worker is part of the isolation contract, and no production reuse exists to
measure today.

All five rows are **fixture-coupled**, though in different ways.
`no_source_does_not_start_worker` times all of `NO_SOURCE` in one `b.iter`
(`crates/aegis-language/tests/common/no_source_corpus.rs`, shared verbatim with
`tests/no_source.rs`), so *adding corpus entries alone* raises the mean with no
actual regression. The four `parse/*` rows each time exactly one snippet
(`bench_with_input` per grammar in `parse_latency_bench.rs`), so they move only
when that grammar's snippet is edited. Either kind of fixture change requires
rebaselining — see the update rules below.

## Threshold policy

- default allowed regression: **+25%**
- `heredoc_worst_case`: **+30%**
- the Iteration 10 slow-path rows: **+25%** on top of an already padded ceiling
- `startup_safe_command`: **+50%**, wider than the other rows because it times
  a whole process invocation rather than in-process work, and a shared CI
  runner's process-spawn variance is larger than its own compute variance.

This is intentionally conservative for the first CI-integrated version. The goal
is to catch meaningful slowdowns without creating noisy failures from normal
runner variance.

If a benchmark exceeds its threshold, `aegis_benchcheck` exits non-zero and
prints a line like:

```text
FAIL safe_command_assess observed 2.200 µs baseline 1.500 µs delta +46.7% threshold +25.0% budget 2.000 ms
```

The `budget` tail only appears for rows that carry a `budget_ns` — `safe_command_assess`, `heredoc_worst_case`, and `startup_safe_command` today.

That output is the primary interpretation surface in CI logs.

### `startup_safe_command` threshold revision rule

The 50% threshold is wide because there is no CI-runner history yet. After ten
consecutive green runs of the `performance` job on `main`, the repository
owner pulls the observed `startup_safe_command` mean from each run's
`benchmark-report.txt` artifact and computes the spread across those ten
values. If the maximum is within 20% of the median, the threshold drops to
25%. The criterion is the spread between runs, not any single figure — one
fast or one slow run says nothing about runner variance on its own.

## Budget ceiling (`budget_ns`)

A policy row may also carry `budget_ns`: an absolute ceiling checked
independently of the delta threshold above. The two checks can disagree — a
regression inside the allowed delta can still land above the budget, and
`aegis_benchcheck` fails the row either way. Only three rows carry one today:
`safe_command_assess` and `heredoc_worst_case` against the 2 ms `Assessment
budget`, and `startup_safe_command` against the 30 ms `Startup cost` ceiling
(ADR-034). The other rows have no per-command or per-invocation promise to
check against, so there is nothing for a budget to bind to.

None of the three budgets binds today — the delta threshold fires first on
every one of them. `heredoc_worst_case`, for example, gates at 1.3 ms
(baseline plus its 30% threshold) against a 2 ms budget, so a regression trips
the delta check well before it could reach the ceiling. A budget only becomes
the binding limit once a rebaseline raises `baseline_ns` closer to it — which
is the reason the field exists: a rebaseline can silently erode the actual
promise unless something still checks it independently of the delta.

## Scheduled job

The CI workflow also exposes a scheduled performance run. Its purpose is to:

- re-check the baseline regularly even without feature work
- leave a benchmark artifact trail in GitHub Actions
- surface drift before release prep

## How to update the baseline

Update the checked-in policy only when:

1. the slowdown is understood and accepted, or
2. the benchmark itself changed in a way that invalidates the old baseline —
   including a change to the fixtures a row times (adding `NO_SOURCE` entries,
   or editing a `parse_latency_bench.rs` snippet).

Recommended update process:

1. run the `cargo bench` invocations listed under "What is checked" that cover
   the affected rows
2. inspect `target/criterion/*/new/estimates.json`
3. adjust `perf/scanner_bench_baseline.toml`
4. explain the reason in the PR description or ticket summary

Do **not** update the baseline just to silence an unexplained regression.

## Iteration 0 — language-aware analysis (ADR-022)

Iteration 0 of the language-aware analysis plan
(`docs/plans/2026-07-16-language-aware-analysis.md`) GREEN list requires six
measurements: clean-build requirements, release binary growth, parse latency,
peak worker RSS, startup cost, and all-target build parity. This section records
each one — measured where it is meaningful in Iteration 0, explicitly deferred
with rationale where it is not — so the budget state is documented rather than
silently missing. Measurements are reproducible by running the cited bench
locally; the date and bench command are the evidence, not a transient capture.

### 1. Clean-build requirements — documented

`aegis-language` pulls in the pinned Tree-sitter runtime plus four crates.io
grammars, each a `build.rs` that compiles bundled C source. That is the
clean-build requirement: a C toolchain (cc) on every supported target. The
4-target cross-compile matrix (see §6) proves the C source builds clean on
musl x86_64/aarch64 and darwin x86_64/aarch64. No numeric clean-build *time* is
recorded: it is runner-dependent and noisy, and the requirement (C toolchain +
the four grammar `build.rs` artifacts) is the actionable fact.

### 2. Release binary growth — measured (zero)

`aegis-language` is a workspace member but is **not a dependency of the shipping
`aegis` binary** in Iteration 0 — nothing in `src/` depends on it. Release
binary growth is therefore **0 bytes**: the shipping binary is byte-for-byte
unchanged by this crate. A growth budget (and the size delta from statically
linking Tree-sitter) becomes meaningful when the crate is linked into the root
binary in a later iteration; until then it is exactly zero.

### 3. Parse latency — measured

- Bench: `cargo bench --bench parse_latency_bench -p aegis-language` (wired
  into the `Performance baseline (scanner bench)` CI job). A *measurement*
  bench — parses one representative inline-source snippet per foundation
  grammar so the recorded latency reflects the slow-path cost an inline
  interpreter target would pay, not a degenerate single-statement parse.
- Measured 2026-07-17 (local, release), mean per parse of a small realistic
  snippet (imports + function + loop + conditional):

  | grammar    | mean parse latency |
  |------------|--------------------|
  | Python     | ~43 µs             |
  | JavaScript | ~25 µs             |
  | TypeScript | ~27 µs             |
  | Bash       | ~18 µs             |

- The no-source path is separate and far cheaper: `no_source_bench` measures
  ~1.03 µs per iteration over a 10-command no-source corpus (~103 ns per
  no-source command), and asserts `Outcome::NotStarted` inside `b.iter` so a
  regression that starts the worker panics the bench.
- Budget: parse latency is a slow-path cost, off the safe-command hot path.
  Iteration 10 adds checked-in `aegis_benchcheck` rows for the no-source corpus
  and each foundation grammar; the Performance baseline CI job fails when a
  recorded mean exceeds its allowed regression. Peak worker RSS and release
  binary-size measurements remain separate qualification evidence.

### 4. Peak worker RSS — deferred

The Iteration 0 worker experiment is in-process and runs only on inline-source
commands; it does not run on the no-source hot path and is not a separate
process. A meaningful peak-RSS budget is defined by the bounded **ephemeral
worker process** (length-bounded framing, crash/hang isolation, typed
degradation), which lands in Iteration 3. Recording a number now would measure
the throwaway in-process helper, not the production worker. Deferred to
Iteration 3.

### 5. Worker start cost — deferred

"Worker start cost" in ADR-022 is the cost of starting the ephemeral worker
process (fork + protocol handshake). There is no worker process in
Iteration 0 — `worker::analyze` is an in-process helper — so there is no
worker start cost to measure. Deferred to Iteration 3, where the bounded
worker process exists. This is not the `Startup cost` defined in
`CONTEXT.md` (ADR-034), which covers the whole process invocation and is
tracked in the "Startup cost" section above.

### 6. All-target build parity — exercised

The `cross-matrix` CI job compiles `aegis-language`'s tests (`--tests`, which
pulls in `grammar_smoke` referencing all four grammars) for each of the four
release targets: musl x86_64/aarch64 and darwin x86_64/aarch64. A grammar that
fails to link on a target fails the job. Parity is therefore exercised as a
build/link gate on all four targets. (Cross targets cannot *execute* the
tests, so runtime parse-parity stays host-only — proven by `grammar_smoke` in
the quality job.)

### 7. Accepted resource budgets — final Iteration 0 defaults

The plan requires Iteration 0 to *replace the hypothesis budget table with chosen
final defaults within ADR-022 ceilings*. The provisional table in the plan
(`docs/plans/2026-07-16-language-aware-analysis.md`) is now superseded by the
accepted defaults below. Each row states its evidence class:

- **measured** — backed by a bench in this document;
- **ceiling-adopted** — accepted as the pre-1.0 default because it *is* an
  ADR-022 hard bound (there is nothing to tune below a bound; a bound needs no
  empirical slope);
- **tune-on-wiring** — accepted as the default now, but its empirical slope is
  re-confirmed by a bench in the iteration that first exercises the governing
  machinery (source reader = Iter 4, recursive queue = Iter 5, worker = Iter 3),
  and must stay within the ceiling.

| Budget                          | Accepted default | Evidence class  | Basis |
|---------------------------------|-----------------:|-----------------|-------|
| No-source worker start          | none (must not start) | **measured** | `no_source_bench` asserts `Outcome::NotStarted`; ~103 ns/command (§3) |
| Existing inline source          | 16 KiB           | ceiling-adopted | preserves the current scanner inline-script limit |
| Script-file default             | 256 KiB          | tune-on-wiring (Iter 4) | global config may tune within the 1 MiB ceiling |
| Script-file hard ceiling        | 1 MiB            | ceiling-adopted | non-configurable (ADR-022) |
| Script files per command        | 8                | ceiling-adopted | project may only tighten |
| Aggregate source per command    | 1 MiB            | ceiling-adopted | project may only tighten |
| Recursive analysis depth        | 8                | ceiling-adopted | hard ceiling for pre-1.0 |
| Total language-analysis time    | 100 ms           | **measured** headroom | per-grammar parse latency is 18–43 µs (§3), so 100 ms is a conservative wall with ~3 orders of magnitude of headroom; re-confirmed as a gated budget when the worker is wired (Iter 3) |

Latency and binary-size budgets are recorded above (§2, §3); the peak-memory
budget is the one deferred to Iteration 3 (§4), where the ephemeral worker
process — the only thing whose RSS is meaningful — first exists. No default here
exceeds an ADR-022 ceiling.

### REVIEW GATE status (ADR-022 §8 + plan Iteration 0)

The Iteration 0 REVIEW GATE requires, before merging a production dependency:
`cargo audit`, `cargo deny check`, all four release builds, license review, and
the grammar security corpus. `aegis-language` is **not linked into the shipping
binary yet**, so the "don't merge a production dependency" clause does not fire
for the shipping `aegis` binary; the gate must be fully green before a later
iteration links the crate. Current status:

| Gate item             | Status                                                                 |
|----------------------|------------------------------------------------------------------------|
| `cargo audit`        | ✓ run 2026-07-17 — 6 advisories, all pre-existing in the opt-in `starlark-policy` feature chain; **none** in tree-sitter or criterion |
| `cargo deny check`   | ✓ run 2026-07-17 — advisories/bans/licenses/sources ok                  |
| Four release builds  | CI-gated via `cross-matrix` (`--tests` per target; link-presence)      |
| License review       | ✓ all four grammars MIT (recorded in `BUILTIN_MANIFEST`; enforced by `deny.toml` permissive-licenses) |
| Grammar security corpus | **OPEN** — not yet built; required before `aegis-language` is linked into the shipping binary |
