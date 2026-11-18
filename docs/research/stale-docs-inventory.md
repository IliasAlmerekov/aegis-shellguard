# Stale agent-read docs and agent config: inventory

Research for ticket #596 (map #591). Checked at commit `06b234d` on `main`.
Read-only pass over tracked files; `landing/` and untracked files are excluded.

## How it was checked

- Every relative Markdown link in tracked `.md` files resolved (none broken).
- Every backticked repo path in docs, config, and scripts was matched against `git ls-files`.
- CLI subcommands, flags, and `AEGIS_*` variables named in docs were matched against `src/main.rs` and the code.
- Dependency versions, crate lists, and module lists were matched against `Cargo.toml`, `crates/`, and `src/lib.rs`.
- Claims about the sandbox, feature flags, and plans were matched against `CHANGELOG.md`, `git log`, and issue state (#229 and #230 are closed).

## Summary

Counts are over the per-file list below (one class per finding).

- safe to delete: 15
- needs a decision: 30
- keep: 22

The five problems with the most weight:

1. `ARCHITECTURE.md` describes a layout that no longer exists (`src/decision/`, `src/interceptor/`, `src/ui/`, `src/snapshot/`, `src/audit/`, `src/config/`). It still calls itself authoritative, and `docs/adr/README.md` points readers to it as the current contract.
2. The mandatory Sandbox (#229, #230) is in `main` and in `CHANGELOG.md` under Unreleased, but five documents still say the code "implements the optional model" and name `sandbox.required` as the switch.
3. `PRD.md:445` and ADR-024 say language analysis ships behind a `language-analysis` cargo feature that is off by default. `Cargo.toml` has no such feature and `aegis-language` is an unconditional dependency (`Cargo.toml:75`).
4. `CONVENTION.md` and `CONTRIBUTING.md` still count 12 crates. `aegis-starlark` was removed (#225), so there are 11 member crates. `CONVENTION.md:138` also says MSRV tracks latest stable, while `Cargo.toml` declares 1.89 and no CI job enforces it.
5. `.agents/pipeline.json` is mostly config for a plugin that `AGENTS.md` says is disabled. Only four keys are read by `pipeline.yml`. The `.semgrep/` rule that `CONVENTION.md:306` says the pipeline gate runs is therefore run by nothing.

## Findings by file

Each line is `file:line` then what is stale, then the class in brackets.

### Root agent entry points

- `AGENTS.md:96-98` — Says `.agents/pipeline.json` "stays" for informational PR checks; accurate, but the plugin it describes is disabled through an untracked file (`.claude/settings.local.json`), so a fresh clone cannot see why. [keep: the pointer is correct and short]
- `AGENTS.md:1-114` and `CLAUDE.md:1-19` — `CLAUDE.md` is a thin pointer to `AGENTS.md`, and `AGENTS.md` points to `CONVENTION.md`. No content is duplicated beyond the pointer. [keep: no drift found]
- `CONVENTION.md:77` — "all 12 library crates are live"; there are 11 member crates (the twelfth was `aegis-starlark`, removed by #225). [needs a decision: fix the count, or drop the count and point at `Cargo.toml`]
- `CONVENTION.md:85`, `CONVENTION.md:105` — "opt-in in 0.x, mandatory in 1.0 (#229)"; #229 is closed and the code is mandatory on `main`. [needs a decision: reword when 0.9.0 ships, or now]
- `CONVENTION.md:138-139` — "MSRV: track latest stable" contradicts `Cargo.toml:40` (`rust-version = "1.89"`) and `CONTRIBUTING.md:35`; "enforce it in CI" is not done (no CI job uses the MSRV; CI pins 1.94.0). [needs a decision: write the real MSRV policy, and decide whether to enforce it]
- `CONVENTION.md:181-186`, `CONVENTION.md:101` — Dependency list says `criterion (0.5)`; `Cargo.toml:101` has `0.8`. Module list at `:98-107` omits `aegis-types`, `aegis-explanation`, and `aegis-language`. [needs a decision: fix versions, or drop version numbers from the rule list]
- `CONVENTION.md:306-307` — Says the agent pipeline gate runs `.semgrep/` rules. The pipeline plugin is disabled, `ci.yml`, `pipeline.yml`, and `scripts/lint.sh` never call semgrep, so nothing runs the rule. [needs a decision: wire it into CI or delete `.semgrep/`]
- `CONTRIBUTING.md:77` — "twelve member crates under `crates/`"; there are 11. [needs a decision: same fix as `CONVENTION.md:77`]
- `CONTRIBUTING.md:175` — Points to GitHub Discussions "once enabled"; the repository has Discussions off (`has_discussions: false`). The same sentence is in the issue templates. [keep: conditional wording is still true; revisit if Discussions stay off]
- `README.md:181` — "Current pre-1.0 implementation: the 0.x binary still records `sandbox_status = "unavailable"` and warns before running unconfined". Not true on `main` after #229. [needs a decision: reword at the 0.9.0 release]
- `README.md:22-27`, `README.md:66-76` — The plugin install block appears twice. Small duplication. [keep: top block is the quick start, lower block is the reference]
- `SECURITY.md:56` and `Cargo.toml:16` — Name a `.planning/` directory that exists nowhere (it is not in `.gitignore` and no file uses it). [safe to delete: remove `.planning/` from both lists]
- `CHANGELOG.md` — Historic entries mention removed paths and features; that is the purpose of a changelog. [keep: history is the point]
- `PRD.md:445` — Says language analysis is behind a `language-analysis` cargo feature, default off, absent from 1.0 binaries. `Cargo.toml` has no such feature; `aegis-language` is linked unconditionally (`Cargo.toml:75`). [needs a decision: the PRD is the normative promise, so either gate the code or amend PRD and ADR-024]
- `PRD.md:1-623` — Otherwise the normative promise; not compared line by line beyond the feature claim. [keep: normative by ADR-027]
- `CONTEXT.md:1-942` — Glossary; no removed term or path found. [keep]

### `ARCHITECTURE.md`

- `ARCHITECTURE.md:3-17` — A banner admits the document is "partially stale" and says to verify every `src/...` path against the tree, while `:17-19` still calls it "authoritative for structural contracts". The two statements contradict each other. [needs a decision: rewrite from the crate layout, or delete and let `CONVENTION.md` §3 carry it]
- `ARCHITECTURE.md:82-107`, `:160-165`, `:201-205`, `:283`, `:297`, `:329`, `:399` — The seven-layer map and section headings name `src/decision/`, `src/interceptor/`, `src/ui/confirm/`, `src/snapshot/`, `src/audit/`, `src/config/`. None exist; `CONVENTION.md:109` says the re-export shims were deleted (#281). [needs a decision: same as above]
- `ARCHITECTURE.md:542-588` — Module-boundary diagram and forbidden-edge table use the removed directories (`decision.rs`, `interceptor/**`, `ui/**`). The tests it cites exist (`tests/main_architecture_slices.rs`, `tests/main_thin_entrypoint.rs`) but enforce the crate DAG instead. [needs a decision]
- `ARCHITECTURE.md:678-692` — "Add a snapshot provider" tells the reader to create `src/snapshot/<name>.rs` and edit `src/snapshot/mod.rs`. The real code is `crates/aegis-snapshot/src/registry.rs`. An agent following this edits files that do not exist. [needs a decision]
- `ARCHITECTURE.md:709-749` — File-size budgets of 1500/2000 lines and a "current breaches" table of `src/snapshot/*.rs` files that were moved. `tests/file_size_budget.rs` enforces 800. [safe to delete: the section is superseded by `CONVENTION.md` and the test]
- `ARCHITECTURE.md:757-780` — Public API list shows 15 modules including `audit`, `config`, `decision`, `interceptor`, `snapshot`, `ui`. `src/lib.rs` exports 9 modules; `CONVENTION.md:112-115` has the right list. [safe to delete: duplicate of `CONVENTION.md`, and wrong]
- `ARCHITECTURE.md:676` — Tells the reader to run `rtk cargo bench`; `rtk` is an agent-side convention per `CONVENTION.md:300`. [keep: harmless]

### `docs/`

- `docs/architecture-decisions.md:1-25` — A "has moved" shim that lists only 9 of the 52 ADRs. Nothing tracked links to it except one old `CHANGELOG.md` line. [safe to delete: its own redirect target is `docs/adr/README.md`]
- `docs/adr/README.md:5-8` — Presents `ARCHITECTURE.md` as "the current structural contracts". [needs a decision: follows the `ARCHITECTURE.md` decision]
- `docs/adr/README.md:21-52` — "All 9 crates are extracted" (there are 11) and a module list full of removed shims (`src/interceptor/parser/`, `src/decision/`, `src/snapshot/`, `src/audit/`). Duplicates `CONVENTION.md` §3. [safe to delete: replace with a link to `CONVENTION.md` §3]
- `docs/adr/README.md:124-130` — Lists two fuzz targets as "current"; nine exist in `fuzz/fuzz_targets/` (see `CONTRIBUTING.md:123`). `CONTRIBUTING.md:133` links to this anchor. [needs a decision: fix the list, or retarget the link]
- `docs/adr/README.md:151-153` — Shared references to `tests/installer_flow.rs` and `tests/full_pipeline.rs`; neither exists (the second was split into `full_pipeline_*.rs`). [safe to delete: both lines]
- `docs/adr/README.md:54-100` — ADR index table; every listed file exists, and ADR-009 is documented as absent. [keep]
- `docs/how-it-works.md:80-86` — "Current pre-1.0 implementation (0.x): the shipped binary still implements the optional model", closing "Closing that gap is #229 and #230"; both are closed. [needs a decision: reword at the 0.9.0 release]
- `docs/threat-model.md:147-153` — Same paragraph: "Until #229 and #230 land, treat the mandatory-layer paragraph as the promise". [needs a decision: same release-time rewrite]
- `docs/threat-model.md:402-413` — "Code and test references" lists `src/decision/`, `src/ui/confirm.rs`, `src/snapshot/mod.rs`, `src/snapshot/git.rs`, `src/snapshot/docker.rs`, `src/audit/logger.rs`, `tests/full_pipeline.rs`; all gone. [safe to delete: the whole list, or replace with crate paths]
- `docs/platform-support.md:55-59` — "The 0.x binary still implements the optional model: the Sandbox is off unless `sandbox.enabled` is set". [needs a decision: same release-time rewrite]
- `docs/config-schema.md:296-326` — Whole section "Current pre-1.0 implementation (0.x)" documents `sandbox.enabled` default `false` and `sandbox.required`, which `aegis-config` now treats as deprecated and ignored (`crates/aegis-config/src/validate.rs`, `deprecated_sandbox_field`). Also checked by `tests/config_schema_docs.rs`. [needs a decision: same release-time rewrite, tests must move with it]
- `docs/config-schema.md:482` — "The `aegis init` template"; the command is `aegis config init` (`src/main.rs:321`). [safe to delete: replace the wrong command name]
- `docs/releases/current-line.md:21` — "does **not** claim sandboxing"; a Sandbox ships in the binary and is mandatory on `main`. [needs a decision: reword with the 0.9.0 notes, or delete the file]
- `docs/releases/current-line.md:1-51`, `docs/releases/v1.0.0.md:1-142` — Version-bound release summaries pinned to `0.8.0`; they go stale at every bump and repeat `README.md` and `CHANGELOG.md`. [needs a decision: keep only `v1.0.0.md`, or generate from `CHANGELOG.md`]
- `docs/performance-baseline.md:535-669` — "Iteration 0" section from the plan: says `aegis-language` is "not linked into the shipping binary yet", defers worker RSS and start cost to Iteration 3 (landed), and cites the removed `starlark-policy` feature (`:665`). Finished plan evidence presented as current. [needs a decision: delete, or move to `docs/history/`]
- `docs/performance-baseline.md:364-366`, `:449` — Paths `benches/no_source_bench.rs`, `tests/no_source.rs` omit the `crates/aegis-language/` prefix; the files exist there. [safe to delete: fix the prefix]
- `docs/release-readiness.md:66-103` — "Language-aware analysis re-entry conditions" restates `docs/language-qualification.md` and the plan, with 11 unchecked boxes for work that is done. [needs a decision: delete, or link to the qualification record]
- `docs/release-readiness.md:207` — Dated v0.5.6 evidence cites `packaging/homebrew/Formula/aegis.rb`, which was deleted. [keep: dated evidence record, not a live instruction]
- `docs/release-readiness.md:374-375` — Cites `scripts/install.js` and `scripts/smoke.js`; they live under `packaging/npm/scripts/`. [keep: dated evidence record]
- `docs/ci.md:1-303` — Job names, tables, and versions match `ci.yml`, `release.yml`, `pipeline.yml`; `tests/supply_chain_ci.rs` pins the numbers. [keep: tested]
- `docs/troubleshooting.md:1-404`, `docs/language-grammar-manifest.md:1-131`, `docs/language-qualification.md:1-45`, `docs/demo/rollback.tape` — No stale path, flag, or variable found. [keep]
- `docs/agents/triage-labels.md:15` — Leftover template line "Edit the 'Label in our tracker' column to match whatever vocabulary you actually use." [safe to delete: the line]
- `docs/agents/domain.md`, `docs/agents/issue-tracker.md` — Match the current process (`-R` flag, `ghsa` remote warning, advisory rule). [keep]

### `docs/plans/`, `docs/superpowers/`, `docs/history/`

- `docs/plans/2026-07-14-m2-custom-regex-limits.md:1-48` — Status "Draft"; the work shipped as #576 (256-byte pattern cap, bounded compile). [safe to delete: finished plan, git history keeps it]
- `docs/plans/2026-07-14-m7-audit-readiness-state.md:1-54` — Status "needs a focused design grill"; no `AuditReady` type exists in `src/shell_flow.rs`. Plan may still describe open work. [needs a decision: promote to an issue and delete, or close as won't do]
- `docs/plans/2026-07-14-p3-follow-ups.md:1-66` — Mixed. P3-4 (renderer fallback, #219), P3-6 (cwd fallback), and P3-7 (Starlark advisories, noted done at `:44`) are shipped; P3-1, P3-2, P3-3, P3-5, P3-8 may be open. Verification list at `:60-66` tells agents to run `rtk cargo test --workspace`, against `CONVENTION.md:284`. [needs a decision: move open items to issues, then delete]
- `docs/plans/2026-07-16-language-aware-analysis.md:1-569` — Its own header says Iterations 1-10 are implemented and the file is "a delivery record". Also repeats the `language-analysis` feature claim (`:9`). Cited by `crates/aegis-language/src/router.rs:13`, `crates/aegis-language/tests/no_source.rs:13`, and `docs/performance-baseline.md:538`. [safe to delete: after those three citations point at ADR-022]
- `docs/plans/2026-07-16-language-aware-analysis.md:256-258`, `:298-301`, `:380` — Planned files `src/analysis/budget.rs`, `tests/language_source_routing.rs`, `crates/aegis-language/src/classifier.rs`, `resolution.rs`, `queue.rs`, `tests/language_javascript_pipeline.rs` do not exist. [safe to delete: with the plan]
- `docs/superpowers/plans/2026-08-05-bash-unicode-fuzz-crash.md:1-` — Single plan for a fixed crash; the corpus file `fuzz/corpus/language_bash/non_ascii_native_scanner_crash` is the kept regression. [safe to delete: the plan, not the corpus file]
- `docs/history/roadmap.md:1-699` — Declares itself history, not scope. Still treats `aegis-starlark` as a "[DONE]" crate (`:344-352`), Phase 5.3 Starlark as "(Done)" (`:480-508`), and says MSRV is "declared and enforced" (`:84-99`, not enforced). Cited by `CONVENTION.md:77`, `:318`, `:322`, `CONTEXT.md:900`, `PRD.md:10`, `docs/adr/README.md:23`, `crates/aegis-sandbox/src/linux.rs:126`, `tests/watch_mode.rs:504`. [needs a decision: the map already asks whether `docs/history` stays; if deleted, nine citations need rewording]

### `.agents/`, `.codex/`, `.claude-plugin/`, `plugins/aegis/`

- `.agents/pipeline.json:3-8`, `:9-26`, `:62-92` — Keys `worktree_dir`, `env`, `gate`, `lanes`, `review`, `auto_merge`, `triage` (with a `{plugin}/scripts/triage_findings.py` command and `.env` path), `test_guard`, `pr_template`, `evidence` are read only by the agent-pipeline plugin, which `AGENTS.md:98` says is disabled. `pipeline.yml` reads only `packages`, `tiers`, `full_run_triggers`, `all_packages`. [needs a decision: trim to the four keys CI reads]
- `.agents/pipeline.json:27-40` — `all_packages` lists 12 names: 11 crates plus `aegis`; correct. `tiers` globs (`src/runtime_gate.rs`, `src/analysis/**`, `src/planning/**`, `src/execution/**`, `src/rollback.rs`, `src/install/**`, `src/update/**`) all exist. [keep]
- `.semgrep/review-finding-ids.yml:1-20` — Rule forbids `R<n>-<n>` pipeline finding ids that live in gitignored `.scratch/`; nothing runs it (see `CONVENTION.md:306`). [needs a decision: wire into CI or delete with the disabled pipeline]
- `.codex/config.toml:1-25` — Tracked, though `.gitignore` ignores `.codex/`; it was force-added. Header comment at `:4-8` accurately lists the rule files. `model_instructions_file = "../AGENTS.md"` resolves. [keep: needed for the Codex baseline]
- `.claude-plugin/marketplace.json:1-13` — Names the `aegis` plugin from `./plugins/aegis`; exists. [keep]
- `plugins/aegis/.claude-plugin/plugin.json:3` — Version `0.8.0` matches `Cargo.toml`. It must be bumped by hand with each release (no test ties the two). [keep: current]
- `plugins/aegis/hooks/aegis-pre-tool-use.sh`, `plugins/aegis/hooks/aegis-session-start.sh` — Near copies of `scripts/hooks/claude-code.sh` and `scripts/hooks/claude-session-start.sh`; the only difference is the `AEGIS_BIN` line (`aegis` versus the `__AEGIS_BIN__` placeholder). Two copies to keep in sync. [needs a decision: generate one from the other, or accept the copy]
- `plugins/aegis/commands/rollback.md:1-16` — The command lists snapshots and restores by id only. It does not mention `aegis rollback --last` (the main promise in `README.md:5`). [needs a decision: add `--last` flow or leave]
- `plugins/aegis/hooks/hooks.json:1-26` — Matches the hook scripts. [keep]

### `.github/`

- `.github/pull_request_template.md:1-10` — Matches the PR body rule in `CONVENTION.md:151`. [keep]
- `.github/ISSUE_TEMPLATE/bug_report.md`, `feature_request.md` — Fine; `feature_request.md:8` and `config.yml:6-8` point to Discussions, which are off. Dead link target for contributors. [needs a decision: enable Discussions or remove the pointers]
- `.github/versions.env`, `.github/dependabot.yml`, `.github/workflows/*.yml` — Consistent with `docs/ci.md`. `ci.yml:294` and `:289-292` cite `tests/no_source.rs` without the `crates/aegis-language/` prefix. [keep: tested; fix the prefix when next editing]

### Other tracked config read by agents

- `.gitignore:5`, `:51-55` — Ignore `.kiro/`, `.gstack/`, `docs/claude-source-borrowings.md`, `docs/CODEX-*.md`, `docs/tickets/`: leftovers of earlier tools and notes, none referenced by tracked files. [safe to delete: the five entries]
- `justfile:3`, `scripts/lint.sh`, `.githooks/pre-push` — Match `CONVENTION.md` §9 and §10. [keep]
