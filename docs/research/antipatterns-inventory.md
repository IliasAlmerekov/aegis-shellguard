# Antipatterns an agent would copy as normal

Research for issue #595 (map #591). Question: which patterns in the code and tests look
normal to an agent but contradict `CONVENTION.md` or each other?

Method: static search only (grep and small scripts, no build). Commit base: `06b234d`.
"Production code" means everything before the first `#[cfg(test)]` / `#[cfg(all(test, ..))]`
in a file, minus files that are test modules (`tests.rs`, `tests/`, `*_tests.rs`,
`test_support.rs`). `landing/` is excluded. `src/` counts as one crate. Counts are from that
scan and can be off by a few where a pattern spans lines.

Classes: **safe to delete**, **needs a decision**, **keep** (with reason).

## Summary

| # | Pattern | Count | Dominant variant | Verdict |
|---|---------|-------|------------------|---------|
| 1 | `unwrap()` / `expect()` in production | 0 unwrap, 8 `expect` (all in `aegis-language`, each with `#[expect(clippy::expect_used, reason)]`) | none; compliant | keep |
| 2 | `panic!` / `unreachable!` in production | 4 `panic!`, 2 `unreachable!` | not covered by `panic_lint_guard` | needs a decision |
| 3 | Test `git` spawns without `GIT_DIR` isolation | 39 of 47 spawn sites (14 files) | unisolated; 8 sites isolated | needs a decision |
| 4 | Test helpers that spawn `aegis` without `HOME` isolation | 0 real cases (1 file spawns the worker only) | isolated via `tests/support::base_command` | keep |
| 5 | Process-wide env or cwd mutation in tests | 34 `set_var`/`remove_var` sites in 5 files, 6 `set_current_dir` sites in 4 files | 3 different guards (mutex, re-exec child, none) | needs a decision |
| 6 | Error type in one layer: `Result<_, String>` vs typed error | 35 `String` results in `src/install/`; 0 elsewhere in `src/` outside clap parsers | typed errors (`AegisError`, `UpdateError`, `WorkerError`) | needs a decision |
| 7 | Stringly typed catch-all error variant | 63 `SnapshotError::Snapshot(String)` uses vs 20 other variant uses | the catch-all | needs a decision |
| 8 | Hand-written error type next to `thiserror` | 1 (`SnapshotError`) vs 10 `thiserror` types | `thiserror` | needs a decision |
| 9 | Home-directory resolution copies | 12 sites, 4 behaviours | `HOME` then `USERPROFILE`, empty filtered (7 sites) | needs a decision |
| 10 | Temp-file-then-rename writers | 7 sites, 3 behaviours | temp + `sync_all` + rename (5 sites) | needs a decision |
| 11 | Duplicated test helpers (`init_git_repo`, `git`, `write_executable`, ...) | ~16 git helper defs, 6 `write_executable` copies; 32 of 94 test files use `tests/support` | each file rolls its own | needs a decision |
| 12 | Result silently dropped (`let _ =`) | 96 in production; 58 are in `aegis-tui` (mostly `queue!`); 6 are `let _ = unused;` | best-effort cleanup or terminal write | keep, 2 exceptions |
| 13 | `.ok()` turning an actionable error into `None` | 47 in production | path probes and parsers of untrusted text | keep, 4 exceptions |
| 14 | Hard-coded exit codes | 17 literals in `src/watch/runner.rs`; `EXIT_INTERNAL` defined twice | named `EXIT_*` constants (~83 uses in 11 files) | needs a decision |
| 15 | `unsafe` without a `// SAFETY:` comment | 16 of ~26 blocks (all in `aegis-sandbox`) | commented in `aegis-config`, `aegis-audit`, `landlock.rs` | needs a decision |
| 16 | Oversized files | 9 files at 790-800 lines; 49 at 700+ | split once at 800 (`tests/file_size_budget.rs`) | needs a decision |
| 17 | Test file names that reuse frozen tracker IDs (`c3_`, `h3_`, `m5_`, `m4`, `l1_`) | 11 files | topic names | needs a decision |
| 18 | Test asserts on a shell script's source text | 1 file (`installer_scripts_atomicity.rs`); 11 files grep workflow YAML | run the script against a fixture | safe to delete (1), needs a decision (11) |
| 19 | `#[allow]` where `expect` is the stated style | 7 `allow`, 11 `expect` | `expect` | needs a decision |
| 20 | Library crates writing to stdout | 0 (only `build.rs` cargo directives) | tracing / stderr | keep |

## Details by crate

### `src/` (the `aegis` crate)

**Error handling (pattern 6).** `Result<_, String>` appears 35 times in `src/install/`:
`claude.rs` 5 (`:52,:64,:80,:133,:190`), `codex.rs` 7 (`:21,:26,:46,:75,:147,:175,:201`),
`mod.rs` 7 (`:51` and others), `shell.rs` 10 (`:23,:92,:151,:162,:166,:174,:206,:214,:240,:246`),
`uninstall.rs` 6 (`:41,:208,:222,:248,:288,:324`). Every other module in `src/` returns
`AegisError`, `UpdateError` (`src/update/error.rs`) or `WorkerError`
(`src/analysis/worker_client.rs:67`). A 36th site, `src/execution/mod.rs:28`, also returns
`String`. `CONVENTION.md` section 5 says library code uses typed errors and `anyhow` only in CLI glue;
`src/install/` is not CLI glue in the usual sense because `src/lib.rs` exposes sibling modules that
are typed. Class: needs a decision (migrate, or document `install` as glue). Kept as is:
`src/main.rs:455,459,463` are clap `value_parser` functions, which need `String` (keep).
`src/bin/*.rs` use `anyhow`, which section 5 allows (keep).

**`map_err(|e| e.to_string())` (same pattern).** 35 in `src/install/`, 4 in
`src/cli_commands.rs:550,622,627,645`. Same decision as above.

**Atomic file replacement (pattern 10).** Seven copies of write-temp-then-rename:
`src/install/mod.rs:73` (no `sync_all`, no `create_new`), `src/install/mod.rs:453`,
`src/install/codex.rs:231`, `src/install/uninstall.rs:346`, `src/install/shell.rs:231`
(no `sync_all`, temp name is only the pid, written with `fs::write`),
`src/update/state.rs:145`, `crates/aegis-config/src/amend/formatting.rs:75`. Five use
`create_new` + `sync_all` + rename, which is the dominant variant. `shell.rs:231` and
`mod.rs:73` are the weaker copies and the ones that rewrite user rc files and hook scripts.
Class: needs a decision (one shared helper).

**Home directory (pattern 9).** `src/analysis/orchestrate.rs:375`, `src/cli_commands.rs:523`,
`src/toggle.rs:141`, `src/install/mod.rs:480`, `src/update/state.rs:83` each resolve
`HOME`, then `USERPROFILE`, rejecting empty values. See also the crate sections for
`aegis-config`, `aegis-audit`, `aegis-snapshot`. Class: needs a decision (one resolver).

**Exit codes (pattern 14).** `EXIT_DENIED/BLOCKED/INTERNAL/ABSENT/INCONCLUSIVE` are defined in
`src/main.rs:367-375` and used about 83 times across 11 files. `src/watch/runner.rs` instead
uses literals: `return 4` at `:67,:74,:87,:116`; `std::process::exit(4)` at
`:178,:452,:492,:516,:634,:733,:759`; `exit_code: 2/3/4` at `:448,:488,:512,:629`.
`aegis_sandbox::EXIT_INTERNAL` (`crates/aegis-sandbox/src/lib.rs:55`) duplicates
`src/main.rs:371`. The exit-code contract is a stated invariant (section 3). Class: needs a
decision. `std::process::exit` in `src/watch/runner.rs` from library code is also unusual next
to the `i32` return style used by the rest of `src/` (same decision).

**Panics (pattern 2).** `src/watch/runner.rs:556` `unreachable!("watch run handles setup
failure first")` is a hidden precondition in a match; `src/install/hook.rs:42` `panic!` is
debug-build-only test injection behind `cfg(debug_assertions)` (keep, documented). Class for
`runner.rs:556`: needs a decision (return the error arm instead).

**Dropped results (pattern 12).** Intentional and commented: `src/install/hook.rs:112,113,123,124`
(a closed pipe must not turn a deny into a crash, keep), `src/analysis/worker_client.rs:465`
and `src/watch/runner.rs:732` (`kill()` on a child already finishing, keep),
`src/update/mod.rs:241,245,257,306` and `src/update/state.rs:146` (lock and temp-file cleanup
on a best-effort path, keep). Unused-variable suppression written as `let _ = x;` instead of
`_x` or removing the parameter: `src/shell_compat.rs:285`, `src/planning/core.rs:194,205`,
`src/planning/prepare.rs:36`, `src/update/mod.rs:258`. Class: safe to delete (rename to `_x`
or drop the binding); these are not errors being discarded.

**`.ok()` that hides an actionable error (pattern 13).** `src/install/claude_plugin.rs:125,167,217`
and `src/install/uninstall.rs:191` turn an unreadable or malformed settings file into
`Missing`/`None`, so `aegis status` cannot tell "absent" from "present but broken". Section 5
says do not convert actionable errors to `None` unless intentional, documented and tested.
Class: needs a decision. Other `.ok()` sites are path probes (`canonicalize`, `current_dir`) or
parsers of untrusted text (keep).

**Process-wide state in tests (pattern 5).** `src/planning/core.rs:437,456,483,502,530,550` set
the process cwd under `CURRENT_DIR_TEST_MUTEX` (`:299`) and restore it by hand.
`src/main_tests.rs:467,489` sets the cwd with no mutex and restores only on the success path.
`src/execution/snapshot_ordering_tests.rs:112` and
`src/watch/runner/tests/approve_always.rs:323` re-run the test as a child process instead.
`src/planning/policy_rules.rs:133,151,157,179,363,437` mutate env vars with unique names and
no lock (safe in practice because the names are unique). Three guards, no shared helper.
Class: needs a decision.

**Test `git` spawns (pattern 3).** Unisolated: `src/main_tests.rs:462`,
`src/planning/core.rs:432,478,524`. Isolated: `src/planning/git_target_plan_tests.rs:20`,
`src/runtime/recovery.rs:457` (test inside `recovery.rs`). Class: needs a decision (see the
`tests/` section for the shared fix).

**Frozen-ID file names (pattern 17).** `src/` has none; see `tests/` and the crates.

### `tests/` (root integration tests, 94 top-level files plus 10 in subdirectories)

**`git` without `GIT_DIR` isolation (pattern 3).** 47 `Command::new("git")` sites across
`src/`, `crates/` and `tests/`. Eight isolate: the canonical `git_command` helper
(`crates/aegis-snapshot/src/git/mod.rs:50`, clears `GIT_DIR` and friends through
`GIT_ENV_VARS_TO_CLEAR`) and seven `env_remove` chains
(`tests/cd_snapshot_target.rs:44`, `git_target_snapshot.rs:21`, `rollback_last.rs:146`,
`sandbox_ceiling.rs:30`, `snapshot_verify.rs:93`, plus `src/` sites above). 39 do not. In
`tests/`: `audit_integrity_wording.rs:44`, `automatic_snapshot_prune.rs:28`,
`config_integration.rs:46,51`, `full_pipeline_json.rs:337`,
`full_pipeline_snapshot.rs:299,379,384,468,473,488,493`, `recovery_degradation.rs:154,160,166`,
`release_workflow_homebrew.rs:129` (sets HOME and git config, not `GIT_DIR`),
`snapshot_integration.rs:46,51,81,86,102,367`, `snapshot_ledger.rs:219`,
`snapshot_ordering.rs:45,52,381,387,486,492,581,587`, `snapshot_prune_git.rs:24`,
`watch_mode.rs:101,107,113`. A hook running in a linked worktree exports `GIT_DIR`, and these
repos then land in the real repository (known issue). The fix lives in one place:
`tests/support/mod.rs` has no git helper, so every file defines its own. Class: needs a
decision (add one isolated helper to `tests/support`, migrate the 14 files).

**`HOME` isolation (pattern 4).** Good. Every file that spawns the `aegis` binary sets `HOME`
through `tests/support/mod.rs:30` `base_command` or its own `.env("HOME", ..)`. The only files
that do not (`analysis_orchestrate*.rs`, `analysis_worker_client.rs`, `language_worker.rs`,
`benchcheck_cli.rs`) spawn the analysis worker or `aegis_benchcheck`, not the CLI. Class: keep.

**Duplicated helpers (pattern 11).** `init_git_repo` is defined in 7 files
(`snapshot_ordering.rs:44`, `automatic_snapshot_prune.rs:19`, `watch_mode.rs:100`,
`config_integration.rs:45`, `snapshot_prune_git.rs:29`, `snapshot_integration.rs:45`,
`recovery_degradation.rs:153`); `git(..)`/`run_git`/`init_repo` in 7 more.
`write_executable` has a shared version at `tests/support/mod.rs:100` and private copies in
`snapshot_integration.rs:34`, `config_integration.rs:34`, `snapshot_rollback_live.rs:45`,
`analysis_orchestrate/runner.rs:47`, `analysis_orchestrate/cli_deadline_parity.rs:44`.
`write_global_config` has the shared one at `support/mod.rs:142` and copies in
`snapshot_integration.rs:73`, `snapshot_ordering.rs:80`, `snapshot_rollback_live.rs:107`.
Only 32 of 94 files declare `mod support`. Class: needs a decision.

**Script source text (pattern 18).** `tests/installer_scripts_atomicity.rs:9` reads
`scripts/install.sh` and `scripts/uninstall.sh` and asserts on substrings such as
`mv "${tmp_rc}" "${rc_file}"`. `CONVENTION.md` section 9 says this proves nothing about what the script does.
`tests/installer_platform.rs` (rc-file assertions at `:54,:88,:133,:183`) and
`tests/uninstall_cli.rs` already run the scripts against fixtures. Class: safe to delete the
file, provided the maintainer confirms those tests cover the rc rewrite. Eleven more files read
workflow YAML text (`supply_chain_ci.rs`, `release_workflow.rs`, `l1_qualification_*.rs`,
`fuzz_ci.rs`, `platform_scope.rs`, `snapshot_rollback_ci.rs`, `feature_ci_coverage.rs`,
`release_install_smoke.rs`, `release_workflow_homebrew.rs`, `npm_package.rs`). The rule names
shell scripts, not YAML, so these are a gray area. Class: needs a decision.

**Oversized files (pattern 16).** `tests/agent_hooks.rs` (800), `tests/claude_plugin.rs` (800),
`tests/full_pipeline_policy.rs` (798) sit at the cap; 14 files in `tests/` are 700+.
The cap is a test (`tests/file_size_budget.rs:16`, `MAX_LINES = 800`) that does not appear in
`CONVENTION.md` or `AGENTS.md`. Files parked at 800 show that the cap shapes how code is
split, not what it contains. Class: needs a decision (document the cap, then split the
files at the cap).

**Frozen-ID names (pattern 17).** `tests/agent_hooks_m4.rs`, `tests/l1_qualification_ci.rs`,
`tests/l1_qualification_contracts.rs`. Section 11 freezes these namespaces and forbids minting
new ones; an agent copying the naming would mint `m6_`. Class: needs a decision.

**Test `.rs` count.** 94 top-level integration test files each link as a separate test binary.
Not a convention breach (keep); noted because adding another follows the dominant layout.

### `crates/aegis-snapshot`

**Error type (patterns 7, 8).** `crates/aegis-snapshot/src/error.rs:4` hand-writes `Display`
and `Error` for `SnapshotError`; the other ten error types in the workspace use `thiserror`
(`aegis-snapshot` does not depend on it). `SnapshotError::Snapshot(String)` (`error.rs:7`) is
the catch-all and is built 63 times, against 20 uses of all typed variants. Class: needs a
decision. `SnapshotError` is `#[non_exhaustive]`, so adding variants is not breaking.

**`panic!` in production (pattern 2).** `crates/aegis-snapshot/src/registry.rs:61`
`_ => panic!("unknown built-in snapshot provider {name:?}")` in `materialize_builtin_plugin`,
fed from the config-supplied name list. If config validation ever lets an unknown name through,
`aegis` panics on the command path. Class: needs a decision (return `SnapshotError::Config`).

**Home directory (pattern 9).** `crates/aegis-snapshot/src/paths.rs:24` does not filter an empty
`HOME`, unlike the dominant variant. `HOME=""` yields a relative snapshot directory. Class:
needs a decision.

**Layout (neighbouring modules).** Five backends use `<name>/mod.rs` + `tests.rs` + `tests/`
(`git`, `docker`, `postgres`, `mysql`; `supabase/mod.rs` + `runtime/`). `sqlite` is
`sqlite.rs` + `sqlite/tests/`, and `registry` is `registry.rs` + `registry/`. The test code in
`sqlite.rs` is inline, not in `sqlite/tests/`. Class: needs a decision (low value, one rename).

**Dropped results.** `mysql/mod.rs:221,222,295,301,305,316,317,333,341,382` and
`postgres/mod.rs:249,255,261` drop `remove_file`/`kill` results on a failure path where the
original error is returned. Class: keep (cleanup on an error path).

**Test `git`/env.** `crates/aegis-snapshot/tests/crate_contract.rs:68,82,92,126` rewrites
process `HOME` to the shared `std::env::temp_dir()` under a mutex (`:23`) and never restores
the original. Class: needs a decision (see pattern 5).

**Oversized.** `registry.rs` (795), `git/tests.rs` (797); six files at 740+. Class: see
pattern 16.

### `crates/aegis-config`

**Home directory (pattern 9).** `model.rs:345` and `:356` and `amend.rs:49` use the dominant
`HOME`/`USERPROFILE`/non-empty form. `ceiling/workspace.rs:247` asks `getpwuid_r` first, then
`HOME`, on purpose for the trusted ceiling (`system_home`, `:255`). Class: keep (security
reason, documented by the function name and tests); the other three are copies of the
dominant variant (needs a decision with pattern 9).

**Process env mutation.** `ceiling/compute_tests.rs:279-289` sets `GIT_WORK_TREE`, `GIT_DIR`,
`GIT_COMMON_DIR`, `TMPDIR` for the whole test process, guarded only by a comment that it is
"the only one in the crate". Class: needs a decision (pattern 5).

**Atomic write.** `amend/formatting.rs:75`, see pattern 10.

**`allow` attributes (pattern 19).** `model/ratchet/direction.rs:21,152`,
`ceiling/workspace.rs:292`. Class: safe to delete the `allow(dead_code)` at `direction.rs:152`
if the item is unused, otherwise convert to `#[expect(.., reason)]`.

**Frozen-ID names (pattern 17).** `model/tests/ratchet/c3_a.rs`, `c3_b.rs`, `c3_residual.rs`.
`CONTEXT.md` defines C3, so these are old references, not new IDs; class: needs a decision.

**Oversized.** `amend.rs` (767), `allowlist.rs` (759); four files at 700+.

### `crates/aegis-audit`

`logger/writer.rs:359` resolves `HOME`/`USERPROFILE`, then falls back to `"."`, so a missing
`HOME` writes the audit log to `./.aegis/audit.jsonl`. Section 7 pins the log at
`~/.aegis/audit.jsonl`, and section 5 forbids silent fallbacks on actionable errors. Class:
needs a decision (fail instead, or document). `logger.rs:43,50` return `Result<_, String>`
from public time parsers (needs a decision, pattern 6). `secure_fs.rs:301` `unsafe` has a
`SAFETY` comment (keep).

### `crates/aegis-sandbox`

**`unsafe` without `// SAFETY:` (pattern 15).** `lib.rs:208,209,224,282,287,288,289,299,319,326,352`
and `bwrap.rs:161,172,181,189,191`. `landlock.rs` has comments on 7 of 10. `CONVENTION.md` is silent on
`unsafe`, so an agent copies whichever neighbour it reads. Class: needs a decision.

**`panic!` in `build.rs:49`.** Intended failure of the build. Class: keep.

**Duplicate constant.** `lib.rs:55` `EXIT_INTERNAL`, see pattern 14.

### `crates/aegis-scanner`

**`panic!`/`unreachable!`.** `patterns.rs:161` is debug-build-only validation of built-in
rules (keep). `patterns.rs:259` `unreachable!("empty prefix rule pattern rejected above")`
relies on a check 25 lines earlier; class: needs a decision (return `ScannerError`, as the
neighbouring arm does at `:251-257`).

**Test env mutation.** `tests/ambient_env.rs` sets and removes 13 env vars in-process
(`CDPATH`, `GIT_DIR`, `SHELLOPTS`, ...). Class: needs a decision (pattern 5).

**Frozen-ID names.** `src/scanner/tests/h3_gaps.rs`, `h3_followups.rs`, `m5_npm.rs`,
`m5_gaps.rs`, `m5_followups.rs`. Class: needs a decision (pattern 17).

**Oversized.** `scanner/prefix_rule.rs` (800), `patterns/builtins_a.rs` (800),
`scanner/tests/edge_cases.rs` (769).

### `crates/aegis-language`

Eight `.expect(..)` in `src/languages/{bash,javascript,typescript,python}.rs`
(`:60,:82`, `:49,:71`, `:50,:72`, `:40,:62`), each behind
`#[expect(clippy::expect_used, reason = ..)]` as section 5 prescribes. Class: keep. The
`benches/parse_latency_bench.rs:85` `expect` is outside the lint's scope (keep).

### `crates/aegis-tui`

`confirm_screen.rs` has 37 and `block_screen.rs` 15 `let _ = queue!(..)`; `shared.rs` 6;
`stdout_renderer.rs:153` 1. Terminal write failures are dropped on a render path with no way
to report them. Class: keep (dominant in the crate, no caller can act on it). Test module
layout is `src/tests/*_tests.rs` (7 files), a third naming style next to `tests.rs` and
`*_tests.rs` beside the module; `decision_tests.rs` is at 800 lines.

### `crates/aegis-parser`, `aegis-policy`, `aegis-explanation`, `aegis-types`

No production `unwrap`/`expect`/`panic!`, no `String` errors, no dropped results found.
Oversized only: `aegis-parser` `embedded_scripts.rs` (783), `segmentation.rs` (772), `lib.rs`
(751); `aegis-policy` `engine/tests/recovery_and_rules.rs` (775). Class: keep.

### Cross-crate: test module layout

Test code lives in four layouts: `tests.rs` (14 files), `tests/` directory (16), `*_tests.rs`
beside the code (64 files, for example `src/main_tests.rs`, `src/setup_shell_parse_tests.rs`),
and inline `mod tests {}`. `CONVENTION.md` section 9 says only "unit tests live near the code
in `#[cfg(test)]` modules". Class: needs a decision (pick one; low cost to leave).

### Cross-crate: `CONVENTION.md` drift

`CONVENTION.md:77` says "all 12 library crates"; `crates/` holds 11 (`Cargo.toml` workspace
members). The 800-line cap is not mentioned. Class: needs a decision (docs only).

## What was not checked

Pub items without `///` docs, tracing-level rule compliance, and whether `src/install` errors
are reachable by `unwrap`-style panics at runtime were not audited. No code was built or run,
so claims about which tests currently fail inside a linked worktree rest on the `GIT_DIR`
leak recorded in project notes, not on a run.
