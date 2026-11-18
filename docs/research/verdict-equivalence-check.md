# Can existing tests prove verdict equivalence for decision-path cleanup?

Research for issue #598, map #591. Read-only investigation of revision 06b234d.

## Short answer

No. Nothing in the repo records a verdict for a command on one revision and compares it with another.

- The tests are assertions written by hand. A cleanup PR that keeps them green proves only that the asserted cases still pass, and those cases were chosen to catch bypasses, not to cover the decision path.
- The one fixture shaped like a corpus, `tests/fixtures/security_bypass_corpus.toml`, holds 75 commands, run in Strict mode, nearly all dangerous. It is the best base for a comparison, but it is too small and too one-sided to stand alone.
- Fuzz targets check that nothing panics. They never look at a verdict.
- Benchmarks check time. They never look at a verdict.
- The CLI already has an evaluation-only mode, `aegis -c <cmd> --output json`, that prints the full verdict without running the command. That makes the smallest addition cheap: a script that runs a command list through two binaries and diffs the JSON.

## What a verdict is

`aegis -c <cmd> --output json` (rendered in `src/policy_output.rs`) prints:

- `risk`, `decision`, `exit_code`, `block_reason`, `decision_source`
- `matched_patterns[]`: `id`, `category`, `risk`, `matched_text`, `description`, `safe_alternative`, `source`
- `allowlist_match`, `snapshot_plan`, `mode`, `ci_state`, `execution.will_execute = false`

It has no explanation text. The human explanation is built separately in `crates/aegis-explanation` and `src/explanation/formatter.rs`, and it is not in the JSON. A before/after check on this JSON covers risk, decision, rule ids and snapshot plan, but not the explanation wording.

I built the release binary and ran the 75-command corpus through it: 2.2 s total, about 30 ms per command, and the output is deterministic for those two spot-checked rows (for example `bash -c 'rm -rf /'` gives `block`, ids `FS-001, EXEC-006, PS-006, LANG-FS-DEL-RF`, `intrinsic_risk_block`, `builtin_pattern`).

## Candidates

### 1. `tests/fixtures/security_bypass_corpus.toml` and `tests/security_regression.rs`

- Where: fixture file plus one test. Run by `cargo test --workspace` in the `quality` job of `.github/workflows/ci.yml`, so it runs in CI on every PR.
- Size: 75 commands.
- Records per command: expected risk, decision, exit code, block reason. The test also asserts only that `matched_patterns` is not empty.
- Misses: rule ids are not pinned, so a cleanup that changes which rule fires still passes. `matched_text`, `description`, `snapshot_plan`, `decision_source` are not checked. Safe, Warn and Danger paths are barely present. Mode is fixed to Strict with empty allowlist, so policy branches (Balanced, Audit, CI, allowlist) are not exercised. It states the expected value rather than recording the current one, so it cannot detect a change in a case it does not list.
- Fit: best existing command list. A before/after script can read its `command` field directly.

### 2. Integration tests in `tests/` (`full_pipeline_*.rs`, `shell_edge_regressions.rs`, `toggle_parity.rs`, `bash_word_parity.rs`, `cd_snapshot_target.rs` and others)

- Where: `tests/*.rs`; about 704 `#[test]` functions across the directory, about 4,700 lines in the `full_pipeline_*` files alone. Run in CI via `cargo test --workspace`.
- Records: hand-written assertions on decision, exit code, JSON fields, audit entries, snapshot plan. Policy, mode, CI and allowlist behaviour is covered here better than anywhere else.
- Misses: commands are inline literals in test bodies (for example the 22-command list in `tests/full_pipeline_json.rs`), so they cannot be extracted as one corpus. Most assert one field, such as `assert_ne!(decision, "auto_approve")`, not the whole verdict.
- Fit: good regression net for config and policy, not usable as a verdict recorder.

### 3. Crate unit tests (scanner, parser, policy, language)

- Where: `#[test]` counts by crate source tree: `aegis-scanner` 434, `aegis-parser` 342, `aegis-policy` 52, `aegis-language` 222, plus 849 in the root `src/`. Run by `cargo test --workspace` in CI.
- Records: per-function expectations, for example scanner assertions in `crates/aegis-scanner/src/scanner/tests/advanced.rs`.
- Misses: they pin internal APIs. A cleanup that moves or deletes a function must edit or delete the test with it, which the map's safety contract already says is allowed. So these tests cannot prove equivalence, because the PR changes them together with the code.
- There is no `crates/aegis-policy/tests/` or `crates/aegis-parser/tests/` directory; the scanner has only `tests/ambient_env.rs`.

### 4. Language corpora (`crates/aegis-language/tests/corpora/`)

- Where: `bash/`, `javascript/`, `python/`, `typescript/` source files (about 520 lines in total, nine categories per language such as `fs_delete`, `exec_shell`, `negatives`, `malformed`), checked by `bash_corpus.rs`, `python_corpus.rs` and similar, with the harness in `tests/common/corpus_harness.rs`. Run in CI via `cargo test --workspace`.
- Records: detected operations (kind, modifiers, certainty, nested payload), with spans deliberately not pinned. This is the adapter output, one layer below the verdict.
- Misses: no risk, decision or rule id. Covers `aegis-language` only, not the scanner, parser, policy or router. Expectations are written in Rust, not stored as data.

### 5. Fuzz targets and corpus (`fuzz/`)

- Where: targets `parser`, `scanner`, `router`, `heredoc`, `language_*`, `language_protocol` in `fuzz/fuzz_targets/`; seed inputs in `fuzz/corpus/<target>/`, 31 files in total (5 scanner, 8 parser, 8 router, 4 heredoc, and one or two per language). Run by the `fuzz` job in `ci.yml` (heavy-job gate: PRs to main, schedule). `tests/fuzz_ci.rs` checks that targets are declared and wired into the job.
- Records: nothing. The scanner target is `let _ = SCANNER.assess(input)`. The router target only touches fields to prove no panic.
- Misses: no verdict, no comparison. The seed corpus is tiny (most files are a single line).
- Fit: useful only as an extra input source. The seed files can be fed to a verdict recorder.

### 6. Benchmarks (`benches/`, `perf/scanner_bench_baseline.toml`)

- Where: `benches/scanner_bench.rs`, `router_bench.rs`, `startup_bench.rs`, plus benches in `crates/aegis-language`. The `performance` job in `ci.yml` runs them and `aegis_benchcheck` compares timings against `perf/scanner_bench_baseline.toml`.
- Records: time per iteration. `router_bench.rs` asserts that planning needs no language worker (`assert_worker_free_plan`), which is a behaviour check, not a verdict check.
- Misses: inputs are about 42 string literals in the scanner bench ("100 dangerous commands" varied across categories) and a handful in the router bench. No outputs are recorded.
- Fit: the "100 dangerous commands" list is another input source. Benchmarks are the right tool when a cleanup might change speed (hot path), and CI already runs them.

### 7. CI jobs that could act as a check

- `quality` (fmt, clippy, `cargo test --workspace`), `fuzz`, `performance`: covered above.
- `.github/workflows/pipeline.yml`: informational checks for test tamper (`scripts/test-tamper-check.sh`), diff coverage of added lines, and `cargo-mutants` on the PR diff. Tamper check shows tests were not weakened and mutants shows tests would notice a changed line, but neither compares verdicts. They are the right companions to a verdict diff, not substitutes.
- No job builds two revisions and compares anything.

## Gaps for a cleanup PR on the decision path

- No recorded verdict exists for Safe, Warn and Danger commands; the one corpus is about blocks.
- No test pins `matched_patterns[].id` per command, so rule attribution can drift silently.
- No test covers non-Strict modes or allowlists as data.
- No check on explanation text (it is outside the JSON).
- Timing: analysis has a 100 ms timeout ceiling, so heavy machine load can degrade a verdict to a timeout result and make a diff flaky. A comparison script must run sequentially, retry any differing row once, and report differences only if they repeat.

## Proposed smallest addition

One shell or Python script, for example `scripts/verdict-diff.sh <base-rev> [<head-rev>]`, no new dependency and no CI change required to start:

1. Build `aegis` from both revisions (`git worktree add` for the base, a separate `CARGO_TARGET_DIR` per tree, per the shared-target note in memory). Release build took 77 s here on a warm machine; the second tree is about the same.
2. Build the command list from files already in the repo: `command` fields of `tests/fixtures/security_bypass_corpus.toml`, `fuzz/corpus/{scanner,parser,router,heredoc}/*.txt` (one command per file or per line), and the dangerous and safe literals in `benches/scanner_bench.rs` and `benches/router_bench.rs`. Add a small hand-written list of safe and warn commands (for example 30 lines) so non-block paths are covered. Total: roughly 150 to 200 commands.
3. For each command run `aegis -c "$cmd" --output json` with a fresh empty `HOME`, `AEGIS_REAL_SHELL=/bin/sh`, `AEGIS_CI=0`, and a workspace config that sets Strict, once per mode of interest (Strict and the default). Reduce the JSON with `jq` to `risk`, `decision`, `exit_code`, `block_reason`, `decision_source`, and sorted `matched_patterns[].id`.
4. Write one TSV per revision and `diff` them. A non-empty diff on a cleanup PR fails the check; a PR that intends a change lists it.

Cost:

- About 100 lines of script, plus a command-list file (the 30-line safe/warn list) that doubles as a permanent corpus.
- Run time: build twice (about 2.5 minutes total on this machine) plus 200 commands times 2 modes times 2 revisions at about 30 ms each, roughly 15 s.
- Not in CI by default. Cleanup PRs run it locally and paste the empty diff into the PR body, matching the map's "must also show unchanged verdicts" rule. Adding it to `pipeline.yml` as an informational job is a later, optional step (one more checkout and build in the job, similar to the existing tamper and mutants jobs).

Optional second step, larger: promote the recorded TSV to a checked-in golden file and a `cargo test` that compares the current binary to it. That catches drift without a second revision, but it needs a rule for updating the golden file and so is a bigger decision. It is not needed for the first cleanup PRs.

Not covered even by the proposal: explanation wording, audit log contents, snapshot behaviour at run time, interactive prompts, and the multi-segment and file-reading cases that depend on filesystem state (script files routed by the router).
