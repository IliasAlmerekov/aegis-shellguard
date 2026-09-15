# ADR-034 — The safe path has two budgets: assessment and startup

## Status

Accepted.

## Context

Aegis runs as a `$SHELL` proxy, so every intercepted command is a new process.
Until now the project stated one performance promise, "safe hot path < 2 ms
(p99)", and enforced it with one benchmark family in `benches/scanner_bench.rs`.
That benchmark builds the `Scanner` outside the measured loop, so the gate only
ever sees warm `assess()` calls.

A measurement of the release binary settled what the untracked part costs.
Isolated `HOME`, evaluation-only mode (`aegis -c 'ls -la' --output json`, which
returns before `shell_flow` and writes nothing), WSL2 host, 200 repetitions:

| What | mean | p50 | p95 |
| --- | ---: | ---: | ---: |
| Full process invocation | 23.3 ms | 22.9 ms | 26.8 ms |
| `aegis --version` (binary load and clap only) | 1.10 ms | 1.02 ms | 1.65 ms |
| `/bin/true` (process-spawn floor) | 0.70 ms | 0.61 ms | 1.11 ms |

Where the time goes, measured cold in a fresh process: tokio runtime 1.3 to
2.5 ms, `prepare_planner` 16.4 to 20.9 ms, planning the command 0.02 ms. Warm,
in a loop: `PatternSet::load` 0.19 ms, `Scanner::try_new` 4.8 ms,
`RuntimeContext::load` 9.1 ms, `prepare_and_plan` 0.001 ms. Cold
`prepare_planner` costs about twice its warm in-loop figure, so a loop-based
microbenchmark understates the one-shot cost by roughly 2x.

Three facts follow. Assessing a command is 0.1% of what an invocation costs;
everything else is construction. The stated `< 2 ms` promise is missed by the
process by about 10x and always has been, because it holds only for the part
the benchmark measures. And Criterion writes `mean`, `median`, `std_dev` and
`MAD` into `estimates.json` and no percentiles at all, so `p99` was not merely
unenforced, it was unenforceable with the tooling in use.

## Decision

Split the promise in two, name both, and state each as a mean.

**Assessment budget** bounds one `assess()` call on any input, including the
worst one. It stays at 2 ms. That number is meaningful as a worst-input ceiling
rather than a typical-command ceiling: the worst inline-script body under
`MAX_INLINE_SCRIPT_LEN` already measures near 1.9 ms, while a typical safe
command measures 2.6 µs.

**Startup cost** is the cost of one process invocation from process start to
decision, before exec. Its budget is recorded at 30 ms against 22 ms measured.
That figure is a recorded fact with headroom, not a product promise and not a
target.

Both terms enter `CONTEXT.md` as glossary entries. The numbers live in `PRD.md`
and in `perf/scanner_bench_baseline.toml`; the glossary carries the definitions
only, so the two cannot drift apart the way the single number did.

The `p99` wording leaves `PRD.md`. The gate compares means, and the documents
say so.

The startup budget is not a 1.0 release gate. Bringing 22 ms down requires
reworking how the scanner and the runtime context are built; gating the release
on a number that is missed today would only postpone the release. The cost of
`Scanner::try_new`, 4.8 ms per invocation, is filed as its own issue.

## Alternatives rejected

**Keep one budget and treat 22 ms as a bug against it.** This leaves a promise
in the PRD that the code has never kept and gives no way to detect a regression
in the meantime: a gate whose baseline already fails reports nothing new.

**Drop the assessment budget as decorative, since assessment is 0.1% of the
invocation.** The 2 ms figure is not decorative on the worst input, where
`heredoc_worst_case` sits near 1.9 ms. Removing it would remove the only bound
on the input-size-driven part of the scan.

**Teach `aegis_benchcheck` to approximate p99 from `median` and `MAD`.** Such an
estimate assumes a distribution the process measurement does not have; 117
voluntary context switches against a 22 ms wall says the process spends much of
its time blocked. A wrong tail estimate is worse than an honest mean.

## Consequences

- `PRD.md` states two budgets as means, and marks the startup figure as
  measured rather than promised.
- `CONTEXT.md` gains **Assessment budget** and **Startup cost**; the **Quick
  scan** entry stops carrying a number of its own.
- `perf/scanner_bench_baseline.toml` gains an optional `budget_ns` per row, so a
  published budget is checked directly instead of being inferred from
  `baseline_ns` plus a percentage. Only rows with a published budget carry it.
- The batch row `1000_safe_commands` is removed. A batch mean of 1,000 commands
  cannot be compared against a per-command budget, and it had been compared
  against it in project documents.
- A benchmark measures one full process invocation, with its own baseline
  captured on the CI runner rather than on a developer machine.

## Measurement provenance

All figures above were captured on 2026-09-15 from a release build on a WSL2
host with 22 cores, using an isolated `HOME`. They are host observations. The
enforced numbers are the checked-in rows in `perf/scanner_bench_baseline.toml`,
whose provenance is recorded in `docs/performance-baseline.md`.
