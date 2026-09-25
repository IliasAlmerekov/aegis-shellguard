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

## 3. Architecture Rules

The repository is a Cargo workspace. The `aegis` binary crate lives at the
root and depends on focused library crates under `crates/` (Phase 4 of
`docs/history/roadmap.md`). Extraction is complete — all 12 library crates are live:
`aegis-types` (zero-dep foundation), `aegis-parser` (shell tokenizer +
`PrefixPattern` matcher), `aegis-scanner` (`Scanner`, `PatternSet`, built-in
patterns), `aegis-policy` (pure `PolicyEngine`), `aegis-config` (config model,
loader, validation, schema, `amend`), `aegis-explanation` (`CommandExplanation`
and related types), `aegis-tui` (crossterm confirmation dialog), `aegis-snapshot`
(six snapshot backends), `aegis-audit` (`AuditLogger`, append-only JSONL with
optional hash-chain integrity), `aegis-starlark` (Starlark policy DSL loader
for `~/.aegis/policy.star`), `aegis-sandbox` (bwrap + Landlock on Linux,
sandbox-exec on macOS; opt-in execution confinement), and `aegis-language`
(the focused Tree-sitter boundary for language-aware analysis — an additive
slow path that owns the grammar manifest and parsing; ADR-022). Dependency
arrows flow inward toward `aegis-types`; no library crate may depend on the
root binary crate. DAG boundaries for the first nine core crates plus the
`aegis-language` leaf are enforced by `tests/architecture_boundaries.rs`
(`aegis-language` must not be depended on by any other workspace member, and
must not itself depend on any other workspace member — ADR-022 §4);
`aegis-sandbox` is covered by `tests/platform_scope.rs`, and `aegis-starlark`
is not yet asserted in either.

Current module responsibilities:

- `src/main.rs`: CLI parsing and orchestration only
- `src/error.rs`: shared typed errors (maps `aegis_scanner::ScannerError` inward)
- `crates/aegis-parser/`: shell tokenizer, segmentation, and `PrefixPattern` matching
- `crates/aegis-scanner/`: command classification, `PatternSet`, built-in patterns, `assess()`
- `crates/aegis-policy/`: pure policy evaluation (`Assessment` + context → decision)
- `crates/aegis-config/`: config model, layered loader, validation, schema, `amend`
- `crates/aegis-snapshot/`: snapshot plugin trait and six backends (git, docker, pg, mysql, sqlite, supabase)
- `crates/aegis-sandbox/`: opt-in execution confinement (bwrap + Landlock / sandbox-exec)
- `crates/aegis-tui/`: interactive confirmation flow (crossterm)
- `crates/aegis-audit/`: `AuditLogger`, append-only JSONL, rotation, optional hash-chain

There are no `src/` re-export shims over these crates (issue #281): every
call site imports a type from the crate that defines it, never through an
intermediate `pub use`. The `aegis` binary crate's own public module list
(`src/lib.rs`, pinned by `public_api_surface_is_stable` in
`tests/architecture_boundaries.rs`) carries only the modules external
consumers use — `analysis`, `error`, `explanation`, `planning`, `runtime`,
`runtime_gate`, `toggle`, `watch` — not a mirror of every library crate.

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
- `RiskLevel` ordering is semantic and must not be changed.
- `Pattern` continues to use `Cow<'static, str>` to support both built-in and user-defined patterns.
- The project must preserve the current exit-code contract.

## 4. Rust and Code Style

- Rust edition: `2024`
- Current MSRV policy: track latest stable during active development
- Production requirement: declare an explicit MSRV and enforce it in CI before claiming production readiness
- Follow standard Rust naming:
  - types / traits / enums: `PascalCase`
  - functions / methods / variables / modules: `snake_case`
  - constants: `SCREAMING_SNAKE_CASE`
  - enum variants: `PascalCase`
  - pattern IDs in data: uppercase strings like `"FS-001"`
- Prefer ASCII unless there is a strong reason not to.
- Keep comments concise and explanatory, not redundant.
- All new public items must have `///` doc comments.
- Avoid broad re-export layers unless they materially improve the public API.
- Commits use the short conventional form (`feat:`, `fix:`, `perf:`, ...), subject line under 72 characters, body explaining why rather than what.
- Every PR body is [`.github/pull_request_template.md`](.github/pull_request_template.md) filled in, at most 10 non-empty lines, whoever opens it; only bot PRs such as Dependabot keep their generated body. With `gh pr create`, copy the template into a file and pass it with `--body-file`. Keep the four parts in order: `Closes #N` (or `No issue: <reason>` for PRs that `CONTRIBUTING.md` lets skip an issue), one or two sentences on what and why, `Evidence:` bullets with each command run and its result (or the test that failed before the fix and passes after), and `Confidence:` as `high`, `medium`, or `low` plus one sentence naming the residual risk. Delete the template comments. File lists and change walkthroughs live in the diff; follow-ups, benchmarks, and session notes go in issue comments.

## 5. Error Handling Rules

- Library code uses typed errors via `thiserror` and `AegisError`.
- `anyhow` is allowed only in CLI glue or top-level application wiring when it simplifies propagation.
- `unwrap()` and `expect()` are forbidden in non-test production paths except for explicit,
  documented startup-time panics where panic is the intended contract. Every library and
  binary crate root enforces this with
  `#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]`, and
  `tests/panic_lint_guard.rs` fails when a crate root lacks the attribute. A sanctioned startup
  panic carries `#[expect(clippy::expect_used, reason = "...")]` on the smallest item or
  statement. Use `expect`, not `allow`, so a stale marker fails the build.
- Errors must not be silently discarded in production logic.
- Do not convert actionable errors into `None` or a silent fallback unless that behavior is
  intentional, documented, and tested.

## 6. Dependency Rules

Approved dependency categories currently include:

- `clap` (4.5, derive API) — CLI parsing.
- `crossterm` (0.28) — terminal UI, confirmation dialog.
- `aho-corasick` (1.1) — fast multi-pattern quick scan.
- `regex` (1.11) — full pattern scan, second pass only.
- `serde` + `toml` (0.8) — config model and parsing.
- `thiserror` — typed errors in library crates.
- `anyhow` — error propagation in CLI glue.
- `tokio` (features: process, fs, rt, rt-multi-thread, io-util, io-std, sync, time) — async subprocess and snapshot work.
- `async-trait` (0.1) — async methods on `dyn Trait` (e.g. `SnapshotPlugin`).
- `tracing` + `tracing-subscriber` — structured logging.
- `criterion` (0.5) — benchmarks.
- `semver` — strict SemVer parsing and comparison for the opt-in update
  notice (ADR-038); pure Rust, no dependencies of its own.
- `cc` (build-dependency) — the C compiler driver for the vendored bubblewrap
  build; **only** `aegis-sandbox` may depend on it (ADR-029 §3–§4).
- `pkg-config` (build-dependency) — locates `libcap` for the vendored bubblewrap
  build; **only** `aegis-sandbox` may depend on it (ADR-029 §3–§4).
- `tree-sitter` (0.26.11) — Tree-sitter runtime; **only** `aegis-language` may
  depend on it (ADR-022 §8). It is the first sanctioned native-C build input.
- `tree-sitter-python` (0.25.0), `tree-sitter-javascript` (0.25.0),
  `tree-sitter-typescript` (0.23.2), `tree-sitter-bash` (0.25.1) — the four
  L1-foundation production-qualified grammars; **only** `aegis-language` may
  depend on them. A grammar is added here only after independent qualification
  (license, maintainer, transitive deps, upstream corpus, fuzzing, all-target
  release builds — ADR-022 §5).
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
- Backward compatibility for older log entries must be maintained where practical.

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

Quality gates:

- `scripts/lint.sh` (rustfmt and clippy, the same commands CI runs)
- `cargo test`
- `cargo audit`
- `cargo deny check`

Pre-push hooks should mirror CI as closely as possible.

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

## 11. Documentation Rules

- Docs must match actual behavior.
- Remove or soften claims that are not supported by tests, benchmarks, or implementation.
- Security docs must state limitations explicitly.
- User-facing docs must not overstate snapshot fidelity, threat coverage, or maturity.
- Changes to public behavior require corresponding doc updates.
- **Tracker ID namespaces are disjoint, frozen, and never reused.** `C<n>`, `H<n>`,
  `M<n>`, and `P3-<n>` are defined in `CONTEXT.md` — the letter is severity.
  `Phase <n>` and `L<n>` refer to `docs/history/roadmap.md`. Both namespaces are
  frozen: neither mints new identifiers, and new work cites its GitHub issue number
  instead. No identifier may carry a second meaning in another document, a test name,
  an assert message, or a source file name. Milestones without a
  `docs/history/roadmap.md` entry are referred to by name, not by an invented ID.
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
- all release-blocking items tracked in the roadmap are complete or explicitly retired
- supported platforms and environment assumptions are documented
- config, audit log, and exit-code compatibility policy is documented
- release workflow is reproducible and verified
- installer verification exists
- dependency and license checks pass in CI
- parser/scanner fuzzing is active or replaced by an explicitly approved equivalent strategy
- threat model and limitations documentation are published
- documentation and implementation are in sync at release time
