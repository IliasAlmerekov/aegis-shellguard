# Aegis Conventions

This document is the project-level contract for code, architecture, security behavior,
tooling, and release readiness in Aegis.

It consolidates the current enforced rules from:

- `docs/adr/README.md`
- `.github/workflows/ci.yml`
- `CONTRIBUTING.md`

If these documents ever disagree, treat this precedence as authoritative:

1. Security invariants and ADRs
2. CI-enforced rules
3. This file
4. Contributor guidance

## 1. Project Scope

Aegis is a Rust CLI that acts as a `$SHELL` proxy and intercepts shell commands
before they reach the real shell.

Its job is to:

- parse and classify shell commands
- require human approval for suspicious or dangerous commands
- hard-block catastrophic commands
- create best-effort snapshots for dangerous commands when configured
- append every decision to an audit log

Aegis is:

- a heuristic command guardrail
- a mandatory OS confinement layer in 1.0 — a write/network guardrail
  ([ADR-029](docs/adr/adr-029-the-sandbox-is-a-mandatory-1-0-layer.md), which
  supersedes ADR-003)
- not a confidentiality boundary and not a privilege boundary
- not a complete security boundary

The project must not claim stronger guarantees than the implementation actually provides.

## 2. Security Invariants

These rules are non-negotiable.

- The deny path must never silently fall through to allow.
- Classification and policy failures must be fail-closed.
- In Aegis, fail-closed means:
  - explicit deny or block, or
  - explicit human approval required before execution.
- Silent auto-approval on scanner, config, confirmation, or policy failure is forbidden.
- `Block`-level commands must never be bypassed by allowlist, CI mode, or refactors.
- Approved commands run with the user's real permissions; code and docs must preserve that model.
- The audit log is a security artifact and must remain append-only.
- Snapshot behavior must be described honestly as best-effort unless fidelity is proven.
- Any change that weakens command interception, confirmation, allowlist safety, CI safety,
  snapshot guarantees, or audit integrity is a security-sensitive change.
- A `tracing` field must never carry a raw command string or an environment variable value.
  The Diagnostic stream is operator-facing stderr, not the append-only Audit log, and it must
  not become a second place secrets or command text leak to. Guarded by a source-grep test
  modeled on `decision_engine_is_pure_no_io` (`tests/architecture_source_rules.rs`); the guard
  cannot see a path that arrives through a `Display` impl on an error type such as
  `SnapshotError`, so it is a floor, not a proof.
- A `tracing` event's level follows one rule, not a fixed list: it is `warn` (or higher) if,
  after it fires, an operator who believes Snapshot coverage is complete would be wrong.
  Normal progress (a snapshot was created, a plugin was not applicable, no container was
  found) stays `info`. Do not mechanically promote every `info` inside an `Err` branch: a
  plugin correctly deciding it has nothing to do is not a coverage degradation.
- A change to how the Aegis `Hook` gets registered names in its ADR every party that can
  switch the `Hook` off, and what the user sees when one does (#500, ADR-047).

## 3. Architecture Rules

The repository is a Cargo workspace; `[workspace].members` in `Cargo.toml` is
the authoritative list of library crates under `crates/`. The `aegis` binary
crate lives at the root and depends on those library crates. Dependency arrows
flow inward toward `aegis-types`; no library crate may depend on the root
binary crate. DAG boundaries for the core crates and the `aegis-language` leaf
are enforced by `tests/architecture_boundaries.rs` (`aegis-language` must not
be depended on by any other workspace member, and must not itself depend on any
other workspace member — ADR-022 §4); `aegis-sandbox` is covered by
`tests/platform_scope.rs`.

Current module responsibilities:

- `src/main.rs`: CLI parsing and orchestration only
- `src/error.rs`: shared typed errors (maps `aegis_scanner::ScannerError` inward)
- `crates/aegis-types/`: zero-dep shared data vocabulary
- `crates/aegis-parser/`: shell tokenizer, segmentation, and `PrefixPattern` matching
- `crates/aegis-scanner/`: command classification, `PatternSet`, built-in patterns, `assess()`
- `crates/aegis-policy/`: pure policy evaluation (`Assessment` + context → decision)
- `crates/aegis-config/`: config model, layered loader, validation, schema, `amend`
- `crates/aegis-explanation/`: `CommandExplanation` and related explanation types
- `crates/aegis-tui/`: interactive confirmation flow (crossterm)
- `crates/aegis-audit/`: `AuditLogger`, append-only JSONL, rotation, optional hash-chain
- `crates/aegis-snapshot/`: snapshot plugin trait and six backends (git, docker, pg, mysql, sqlite, supabase)
- `crates/aegis-sandbox/`: execution confinement (bwrap + Landlock on Linux, sandbox-exec on macOS; mandatory in 1.0 per ADR-029 and #229)
- `crates/aegis-language/`: isolated Tree-sitter boundary for additive language-aware analysis; owns the grammar manifest and parsing (ADR-022)

There are no `src/` re-export shims over these crates (issue #281): every
call site imports a type from the crate that defines it, never through an
intermediate `pub use`. The `aegis` binary crate's own public module list
(`src/lib.rs`, pinned by `public_api_surface_is_stable` in
`tests/architecture_boundaries.rs`) carries only the modules external
consumers use — `analysis`, `error`, `explanation`, `planning`, `runtime`,
`runtime_gate`, `toggle`, `update`, `watch` — not a mirror of every library crate.

Architectural constraints:

- `src/main.rs` must remain thin; business logic belongs in focused modules.
- Import each type from the crate that defines it — never re-export a crate's
  type through a `src/` module just to shorten the path.
- The scanner is the hot path and must stay synchronous.
- Async is allowed for subprocess and snapshot operations, not for parser/scanner logic.
- Blocking work on an async path (subprocess spawn/wait, sandbox probes) goes through `spawn_blocking`, never straight onto the async runtime thread.
- A trait with an async method used as `dyn Trait` carries `#[async_trait]`, because a native `async fn` in a trait is not object-safe.
- Library crates under `crates/` never write to stdout; they emit `tracing` events or write to stderr. Only the root `aegis` crate writes to stdout, for example the watch-mode NDJSON frames in `src/watch/protocol.rs`.
- Quick scan must remain Aho-Corasick based.
- Full regex evaluation must remain on the slower second pass only.
- Quick scan never under-matches the full regex set: a pattern with no extractable keyword forces the quick scan to report a match, so the full scan still runs.
- `RiskLevel` ordering is semantic and must not be changed. `RiskLevel` is `#[non_exhaustive]`, so matches outside its crate need a wildcard arm.
- `Pattern` continues to use `Cow<'static, str>` to support both built-in and user-defined patterns.
- Use `aegis-types` `EXIT_*` constants for reserved Aegis exit codes; preserve the exit-code contract.
- `aegis-policy` stays a pure function: its production code never names `std::fs`, `std::process`, `tokio::`, `std::io`, `std::env`, `tracing::`, `eprintln!`, or `println!` (`decision_engine_is_pure_no_io`).
- `aegis-tui` only renders: it never depends on `aegis-audit`, `runtime`, or `planning`, and never calls Snapshot logic. The `SnapshotRecord` display type is the one allowed import.
- In production code only `src/planning` calls `evaluate_policy`; transports go through `planning::*` (`tests/architecture_source_rules.rs`).
- An installer decides whether to write a settings file only from the scopes that file
  covers. The full precedence chain is for reporting, as in `aegis status` (#500).

## 4. Rust and Code Style

- Rust edition: `2024`
- MSRV: `rust-version` in `Cargo.toml` (1.89 today), raised only on purpose with a `CHANGELOG.md` entry
- Follow standard Rust naming:
  - types / traits / enums: `PascalCase`
  - functions / methods / variables / modules: `snake_case`
  - constants: `SCREAMING_SNAKE_CASE`
  - enum variants: `PascalCase`
  - pattern IDs in data: uppercase strings like `"FS-001"`
- Prefer ASCII unless there is a strong reason not to.
- Keep comments concise and explanatory, not redundant.
- Every `unsafe` block carries a `// SAFETY:` comment directly above it naming the invariant that makes the call sound, enforced by `clippy::undocumented_unsafe_blocks` through `[workspace.lints]`.
- All new public items must have `///` doc comments.
- Avoid broad re-export layers unless they materially improve the public API.
- Commits use the short conventional form (`feat:`, `fix:`, `perf:`, ...), subject line under 72 characters, body explaining why rather than what.
- Every PR body is [`.github/pull_request_template.md`](.github/pull_request_template.md) filled in, at most 10 non-empty lines, whoever opens it; only bot PRs such as Dependabot keep their generated body. With `gh pr create`, copy the template into a file and pass it with `--body-file`. Keep the four parts in order: `Closes #N` (or `No issue: <reason>` for PRs that `CONTRIBUTING.md` lets skip an issue), one or two sentences on what and why, `Evidence:` bullets with each command run and its result (or the test that failed before the fix and passes after), and `Confidence:` as `high`, `medium`, or `low` plus one sentence naming the residual risk. Delete the template comments. File lists and change walkthroughs live in the diff; follow-ups, benchmarks, and session notes go in issue comments.
- A fix unrelated to the PR's issue ships in its own PR, so reverting the issue's PR
  reverts only that issue's work.

## 5. Error Handling Rules

- Library code uses typed errors via `thiserror` and `AegisError`.
- `anyhow` is allowed only in CLI glue or top-level application wiring when it simplifies propagation.
- Production code returns an error or makes the failing case unrepresentable (an enum with an
  exhaustive `match`, a value matched once and passed on) instead of calling `unwrap()`,
  `expect()`, `panic!` or `unreachable!`. Every library and binary crate root enforces this with
  `#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable))]`,
  and `tests/panic_lint_guard.rs` fails when a crate root lacks the attribute. An exception is an
  explicit, documented startup-time panic where panic is the intended contract, or a
  `cfg(debug_assertions)` check. It carries `#[expect(clippy::expect_used, reason = "...")]` or
  `#[expect(clippy::panic, reason = "...")]` on the smallest item or statement. Use `expect`, not
  `allow`, so a stale marker fails the build. Build scripts are not crate roots and stay outside
  the lint.
- Errors must not be silently discarded in production logic.
- Do not convert actionable errors into `None` or a silent fallback unless that behavior is
  intentional, documented, and tested.
- Every deliberate `let _ =` or `.ok()` (including `.ok()?` and chains) in production code carries a
  one-line why comment directly above the drop (or in the enclosing function's doc comment naming
  the drop); name unused bindings `_x` at introduction, keeping `let _ = x;` with a why comment only
  when `#[cfg]` makes the binding used on another target.

## 6. Dependency Rules

Approved dependency categories (exact versions live in `Cargo.toml` and each
member crate's `Cargo.toml`):

- `clap` (derive API) — CLI parsing.
- `crossterm` — terminal UI, confirmation dialog.
- `aho-corasick` — fast multi-pattern quick scan.
- `regex` — full pattern scan, second pass only.
- `serde` + `toml` — config model and parsing.
- `thiserror` — typed errors in library crates.
- `anyhow` — error propagation in CLI glue.
- `tokio` — async subprocess and snapshot work.
- `async-trait` — async methods on `dyn Trait` (e.g. `SnapshotPlugin`).
- `tracing` + `tracing-subscriber` — structured logging.
- `criterion` — benchmarks.
- `semver` — strict SemVer parsing and comparison for the opt-in update
  notice (ADR-038); pure Rust, no dependencies of its own.
- `cc` (build-dependency) — the C compiler driver for the vendored bubblewrap
  build; **only** `aegis-sandbox` may depend on it (ADR-029 §3–§4).
- `pkg-config` (build-dependency) — locates `libcap` for the vendored bubblewrap
  build; **only** `aegis-sandbox` may depend on it (ADR-029 §3–§4).
- `tree-sitter` — Tree-sitter runtime; **only** `aegis-language` may
  depend on it (ADR-022 §8). It is the first sanctioned native-C build input.
- `tree-sitter-python`, `tree-sitter-javascript`, `tree-sitter-typescript`,
  `tree-sitter-bash` — the L1-foundation production-qualified grammars; **only**
  `aegis-language` may depend on them. A grammar is added here only after
  independent qualification (license, maintainer, transitive deps, upstream
  corpus, fuzzing, all-target release builds — ADR-022 §5).
- bubblewrap — the second sanctioned native-C build input, vendored under
  `crates/aegis-sandbox/vendor/bubblewrap/` at a pinned version and compiled
  into the embedded `bwrap` fallback; **only** `aegis-sandbox` may build it
  (ADR-029 §3–§4). It is `LGPL-2.0-or-later`, which `cargo deny` cannot see
  because vendored C is not a cargo dependency — so it is recorded in
  `THIRD_PARTY_NOTICES.md` and enforced by the contract test that reads the
  vendored sources, not by cargo-deny.

Compiling the workspace on Linux needs `libcap` headers (`libcap-dev` on
Debian/Ubuntu): the `aegis-sandbox` build script compiles the vendored
bubblewrap C sources and probes `libcap` via `pkg-config`, failing the build
when the headers are absent (ADR-029 §3–§4). CI installs them on every
Linux-compiling job through `.github/actions/install-libcap`. Two escape
hatches exist for local builds only; neither may be used in CI:

- `AEGIS_SKIP_BWRAP_BUILD=1` skips the C build entirely. The resulting binary
  has no embedded `bwrap` fallback, so confinement then needs a usable system
  `bwrap` on `PATH`.
- `AEGIS_BWRAP_SOURCE_DIR=<path>` builds against an alternative bubblewrap
  source checkout instead of `crates/aegis-sandbox/vendor/bubblewrap/`.

Dependency rules:

- Prefer the standard library where it is sufficient.
- Do not add new dependencies without clear justification.
- `once_cell` is banned; use `std::sync::LazyLock`.
- Avoid dependencies that add unnecessary portability or build complexity.
- Native C build inputs are forbidden except for the pinned Tree-sitter runtime
  and production-qualified generated grammars governed by ADR-022, and the
  vendored bubblewrap C sources governed by ADR-029 §3–§4. Any other native
  dependency requires a separate ADR and supported-target build evidence.
- Supply-chain health is part of project correctness, not an optional extra.

## 7. Configuration and Audit Contracts

Configuration rules:

- User config is TOML.
- Effective config is layered from:
  - project `.aegis.toml`
  - global `~/.config/aegis/config.toml`
  - built-in defaults
- New config fields must preserve backward compatibility.
- New fields are optional via `#[serde(default)]`, so older config files keep loading.
- Config changes must be documented and tested for merge semantics.

Audit rules:

- Audit output remains append-only JSONL at `~/.aegis/audit.jsonl`.
- New entries use RFC 3339 / ISO 8601 timestamps with timezone.
- Audit querying must remain compatible with rotated archives.
- Machine-readable audit output is part of the public contract.
- Keep readers of data Aegis persisted forever; keep input shapes that may live in a user's files or scripts (removing one requires a breaking-change decision, never cleanup); remove dead compatibility layers.

## 8. Performance Rules

- Safe-command overhead is performance-sensitive and should stay under the project target.
- Hot-path changes in parser/scanner must minimize allocation and cloning.
- Avoid unnecessary heap work in classification code.
- Do not introduce async overhead into the interception hot path.
- Performance-sensitive changes should be benchmarked against `benches/scanner_bench.rs`.

## 9. Testing and Verification

Minimum expectations:

- Unit tests live near the code in `#[cfg(test)]` modules where appropriate.
- Integration tests live under `tests/`.
- Parser changes must add edge-case coverage.
- Classification changes must add positive and negative cases.
- Policy and confirmation changes must add fail-closed regression coverage.
- CI/non-interactive behavior changes must add no-TTY coverage.
- Snapshot changes must add lifecycle and rollback tests.
- Security-sensitive behavior must be regression-tested before merge.
- Hot-path behavior must be checked for performance regressions when parser/scanner logic changes.
- A test of a shell script runs the script, or the block under test, against a fixture.
  An assert on the script's source text proves nothing about what it does.
- `tests/evaluation_golden.rs` pins the verdict of a fixed command corpus in `tests/fixtures/evaluation_golden.jsonl`.
  After a change that alters a verdict, regenerate it with `AEGIS_UPDATE_EVALUATION_GOLDEN=1 cargo test --test evaluation_golden` on an otherwise idle machine.
  A behaviour-changing PR commits the new golden in the same PR; a cleanup PR never edits it.

Quality gates, all run by CI and required to merge:

- `scripts/lint.sh` (rustfmt, clippy, and `scripts/lint.sh text`, the grep-and-find checks clippy cannot make, such as the 800-line `.rs` limit)
- `cargo test --workspace`
- `cargo audit`
- `cargo deny check`

The pre-push hook runs only `scripts/lint.sh fmt`, which compiles nothing. Nothing else runs locally before a push. Locally, run only the targeted test for the code you changed, for example `cargo test --test full_pipeline_policy` or `cargo test -p aegis-parser`. A full local build costs tens of gigabytes of `target/` per checkout.

Production-level verification requirements:

- parser and scanner fuzz targets must exist and be maintained
- complex shell parsing behavior should receive regression tests and fuzz coverage
- performance-sensitive changes should be measured against an explicit budget
- release candidates should be validated on the supported platform matrix
- critical public contracts must have backward-compatibility tests where practical

## 10. CI and Local Tooling

CI currently enforces formatting, linting, tests, security audit, dependency policy, and release build.

Local development rules:

- `rtk` is an agent-side execution convention used to reduce context-window noise during AI-assisted sessions.
- `rtk` is not part of the runtime product contract and not a requirement for end users or normal project builds.
- Do not rely on local-only shortcuts that bypass CI checks.
- Keep CI, docs, and local contributor instructions aligned.
- A script that release CI runs and that edits a tracked file reaches users only if the
  workflow commits the file. Docs must not claim such an edit ships.

## 11. Documentation Rules

- Docs must match actual behavior.
- Remove or soften claims that are not supported by tests, benchmarks, or implementation.
- Security docs must state limitations explicitly.
- User-facing docs must not overstate snapshot fidelity, threat coverage, or maturity.
- Changes to public behavior require corresponding doc updates.
- **Tracker ID namespaces are disjoint, frozen, and never reused.** `C<n>`, `H<n>`,
  `M<n>`, and `P3-<n>` are defined in `CONTEXT.md` — the letter is severity.
  `Phase <n>` and `L<n>` are Roadmap milestone IDs, found in old commits and issues.
  Both namespaces are frozen: neither mints new identifiers, and new work cites its
  GitHub issue number instead. No identifier may carry a second meaning in another
  document, a test name, an assert message, or a source file name. Other milestones
  are referred to by name, not by an invented ID.
- Release gating lives in the `1.0` milestone
  ([ADR-027](docs/adr/adr-027-one-1-0-release-gate-lives-in-the-issue-tracker.md)); the
  open supply-chain items (SBOM, signing, `cargo publish --dry-run`) are tracked in
  issue #414.

## 12. Change Review Heuristics

Treat a change as high-risk if it touches any of:

- `src/main.rs`
- `crates/aegis-parser/`
- `crates/aegis-scanner/`
- `crates/aegis-policy/`
- `crates/aegis-config/`
- `crates/aegis-tui/`
- `crates/aegis-explanation/`
- `crates/aegis-audit/`
- `crates/aegis-snapshot/`
- `crates/aegis-sandbox/`
- `Cargo.toml`
- `Cargo.lock`
- CI workflows
- installer or release automation

High-risk changes must be reviewed for:

- fail-closed behavior
- allowlist bypasses
- `Block` semantics
- audit integrity
- backward compatibility
- documentation drift
- supply-chain impact

## 13. Production Readiness Criteria

The project can be described as production-ready only when all of the following are true:

- all current security invariants in this document are implemented and tested
- every issue in the `1.0` milestone ([ADR-027](docs/adr/adr-027-one-1-0-release-gate-lives-in-the-issue-tracker.md)) is closed or explicitly retired
- supported platforms and environment assumptions are documented
- config, audit log, and exit-code compatibility policy is documented
- release workflow is reproducible and verified
- installer verification exists
- dependency and license checks pass in CI
- parser/scanner fuzzing is active or replaced by an explicitly approved equivalent strategy
- threat model and limitations documentation are published
- documentation and implementation are in sync at release time
