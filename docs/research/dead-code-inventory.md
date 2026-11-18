# Dead code inventory

Ticket: #592 (map #591). Scope: the Rust workspace (`src/` as one crate, `crates/*`, `tests/`, `benches/`, `fuzz/`, `scripts/`, `docker/`, `packaging/`, `perf/`, CI). `landing/` is excluded. Snapshot: `main` at 06b234d.

## Method

- Static search only; nothing was built. rustc already fails CI on `dead_code` for private items, so the residue is public items, which rustc never flags.
- Collected every `fn`, `struct`, `enum`, `trait`, `type`, `const`, `static` and enum variant. Counted word occurrences in production code (test files, `mod tests` blocks, comments and `use` lines removed) and again across the whole workspace including tests, benches and fuzz. An item with no production caller was then checked by hand with grep, including `cfg(test)`, `cfg(feature = "testing")` and `cfg(target_os)` gates.
- Cargo dependencies and features, `#[allow(dead_code)]` and `#[allow(unused...)]` suppressions, test helpers, scripts and CI wiring were checked separately.
- Limit: a name-based count can miss an item whose name is shared with another item, and it cannot see dead code reached only by other dead code. Treat the lists as high-confidence, not exhaustive.

## Summary

The repository is clean at the private level. What remains is small:

- Safe to delete: 6
- Needs a decision: 16
- Keep (with reason): 22

Most "needs a decision" items are public functions kept alive only by tests. Every crate is internal to one product, so each is a real candidate: delete the function and its tests together, or demote it to `#[cfg(test)]`.

No unused test helper was found. No orphaned script was found except one test script. The only Cargo feature besides `default = []` is `testing`, and it is used.

## Per crate

### src/ (crate `aegis`)

- `src/analysis/queue.rs:149` `PushOutcome::is_accepted`: public method with zero callers anywhere (src, crates, tests, fuzz, benches). Class: safe to delete.
- `src/runtime/context.rs:152` `RuntimeContext::new_with_audit_path`: `#[cfg(test)]`, used by three test files. Class: keep (test seam, already gated).
- `src/runtime/context.rs:173` `set_snapshot_registry_for_tests`: `#[cfg(test)]`, used by tests. Class: keep.
- `src/explanation/formatter.rs:82` trait `CommandExplanationExt`: one implementor, one production caller (`src/runtime/context.rs:647`). Class: keep (live).
- `src/rollback.rs:350` `Restoring`: test plugin inside the test module. Class: keep.
- `src/analysis/recursive.rs:52` `#[allow(clippy::too_many_arguments)]`: lint relaxation, not dead code. Class: keep.

### aegis-types

- `crates/aegis-types/src/analysis.rs:290` `TargetAnalysis`: public struct re-exported in `lib.rs:28`, constructed only in `analysis/tests.rs`. No production code or other crate reads or builds it. Class: needs a decision (wire type, maybe reserved for a protocol).
- `crates/aegis-types/src/analysis.rs:225` `SourceOrigin::Heredoc` and `:231` `SourceOrigin::Pipe`: never constructed outside tests; `src/analysis/orchestrate.rs:237-255` builds only `Inline`, `ScriptFile` and `Stdin`. The enum is serialised, so removal changes the wire format. Class: needs a decision.
- `crates/aegis-types/src/policy.rs:46,52,61` `expecting`, `visit_str`, `visit_map`: serde `Visitor` trait methods with no direct caller by design. Class: keep (trait implementation).

### aegis-parser

- `crates/aegis-parser/src/prefix_match.rs:57` `matches_prefix_ending_options`: re-exported, called only from its own unit tests (the scanner uses `matches_prefix_with`). Class: needs a decision.

### aegis-scanner

- `crates/aegis-scanner/src/scanner/mod.rs:325` `Scanner::prefix_scan`: public, called only from `scanner/tests/edge_cases.rs`; production uses `prefix_scan_effective_slices`. Class: needs a decision.
- `crates/aegis-scanner/src/scanner/prefix_rule.rs:19` `PrefixRule::matches_tokens`: called only in that file's unit tests; production uses `matches_tokens_with`. Class: needs a decision.
- `crates/aegis-scanner/src/scanner/mod.rs:386` `try_new_call_count_for_tests`: `#[doc(hidden)]` counter read by `src/runtime/context/tests.rs`. Class: keep (documented cross-crate test hook).

### aegis-config

- `crates/aegis-config/Cargo.toml:20` dependency `tracing`: no `tracing` use anywhere in the crate. Class: safe to delete.
- `crates/aegis-config/src/validate.rs:412` `validation_load_error`: re-exported at `lib.rs:44`, used only by `validate/tests.rs`. Class: needs a decision.
- `crates/aegis-config/src/allowlist/analysis.rs:37` `analyze_blocklist_rule`: re-exported at `lib.rs:27`, used only by tests in `allowlist.rs`. Class: needs a decision.
- `crates/aegis-config/src/validate.rs:56` `ConfigSourceMap::for_config`: used only by `validate/length_tests.rs`. Class: needs a decision (demote to `#[cfg(test)]` or delete).
- `crates/aegis-config/src/model/merge.rs:28` `AegisConfig::merge_layer`: no production caller (`merge_layer_all_warnings` is the live path); used by `model/tests/deser.rs`. Class: needs a decision.
- `crates/aegis-config/src/model/ratchet/direction.rs:152` `#[allow(dead_code)]` on `Unratcheted::reason`: field is written and never read, documented as "not read at runtime". Class: needs a decision (documentation stored as data).
- `crates/aegis-config/src/model/ratchet/direction.rs:21` `#[allow(clippy::too_many_arguments)]`: lint relaxation. Class: keep.
- `crates/aegis-config/src/model/ratchet/warning.rs:51` `cfg_attr(not(test), allow(unused_variables))`: parameter used only under `cfg(test)`. Class: keep.
- `crates/aegis-config/src/ceiling/workspace.rs:292` `#[allow(unused_mut)]`: `dirs` is pushed to only under `cfg(target_os = "macos")`. Class: keep.
- `crates/aegis-config/src/allowlist.rs:66,203` `with_user`, `is_allowed`: already `#[cfg(test)]`. Class: keep.

### aegis-audit

- `crates/aegis-audit/Cargo.toml:14` dependency `aegis-scanner`: no use in `src/`. Class: safe to delete.
- `crates/aegis-audit/Cargo.toml:30` dev-dependency `aegis-scanner`: no use in `tests/` or unit tests. Class: safe to delete.
- `crates/aegis-audit/src/logger/writer.rs:247` `AuditLogger::with_path`: no production caller; used by `tests/public_api.rs` and `tests/audit_v2.rs`. Class: needs a decision.

### aegis-policy

- `crates/aegis-policy/Cargo.toml:14` dependency `aegis-scanner`: no `aegis_scanner` reference in the crate; the crate `description` (line 10) still says it depends on the scanner. Class: safe to delete (and fix the description).

### aegis-explanation

- `crates/aegis-explanation/src/lib.rs:80` `concise_reason_label`: used by `aegis-tui/src/shared.rs`. Class: keep (live).

### aegis-tui

- `crates/aegis-tui/src/execution_renderer.rs:113` `set_recovery_decision`: public, zero callers anywhere. Class: safe to delete.
- `crates/aegis-tui/src/execution_renderer.rs:106` `set_confirmation_decision`: called only from `src/execution/snapshot_ordering_tests.rs`. Class: needs a decision (belongs behind `#[cfg(test)]` or a test double).

### aegis-snapshot

- `crates/aegis-snapshot/src/retention.rs:34` `RetentionPolicy::from_max_age_days` and `:42` `from_max_count_per_provider`: only called from `lib.rs` unit tests. Class: needs a decision.
- `crates/aegis-snapshot/src/registry.rs:188` `SnapshotRegistry::new_with_plugins` and `Cargo.toml:29` feature `testing`: gated by `cfg(any(test, feature = "testing"))`, enabled by the root dev-dependency (`Cargo.toml:99`), used by `src/` tests. Class: keep.
- `crates/aegis-snapshot/src/supabase/runtime/manifest_state.rs:90,106` `phase1_empty`, `phase1_fixture`: `#[cfg(test)]`. Class: keep.
- `crates/aegis-snapshot/src/lib.rs:84` `SNAPSHOT_FAILED_CONTINUING`: used by `registry.rs:326`. Class: keep (live).

### aegis-language

- `crates/aegis-language/src/language.rs:38` `SourceLanguage::from_id`: called only by `tests/grammar_smoke.rs`. Class: needs a decision.
- `crates/aegis-language/src/manifest.rs:17,40,125,157` `RELEASE_GRAMMARS`, `RELEASE_TARGETS`, `validate_manifest`, `BUILTIN_MANIFEST`: only `tests/grammar_smoke.rs` and `docs/language-grammar-manifest.md` use them; no build or CI step does. The manifest is a documented release contract (ADR-022 section 8). Class: needs a decision (keep as a tested contract or move under `cfg(test)`).

### aegis-sandbox

- `crates/aegis-sandbox/src/support.rs:64,73` `OmittedRoot`, `ResolvedAllowWrite`: live through `resolve_allow_write`. Class: keep.
- `crates/aegis-sandbox/Cargo.toml` build-dependencies `cc`, `pkg-config`: used by `build.rs` (a `src/` grep misses them). Class: keep.
- `cfg(target_os = "macos")` code (`macos.rs`, parts of `lib.rs` and `support.rs`): every non-test function has a caller; nothing dead found. Class: keep.

No findings in `aegis-types` beyond those above and none in the other crates not named here.

### Cross-cutting: tests, scripts, packaging, features

- `tests/support/mod.rs:13` and `tests/support/installer.rs:10` `#![allow(dead_code)]`: each integration test file compiles its own copy of the helper module and uses a subset. Every helper in `tests/support/*` and `tests/common/mod.rs` has at least one user. Class: keep.
- `tests/architecture_source_rules/walker.rs:415`: the text `#[allow(dead_code)]` is a string fixture, not a suppression. Class: keep.
- `scripts/test-tamper-check.test.sh`: self-test of `scripts/test-tamper-check.sh`. No workflow, `justfile` or doc runs it. Class: needs a decision (wire into CI or delete).
- `scripts/setup-git-hooks.sh`: referenced only by `CONTRIBUTING.md`, for manual use. Class: keep.
- All other scripts (`agent-setup.sh`, `install.sh`, `uninstall.sh`, `lint.sh`, `stage-npm-notices.sh`, `test-tamper-check.sh`, `update-*.sh`, `hooks/*`) are referenced by tests, workflows or source. Class: keep.
- `Cargo.toml:62` `default = []`: empty default feature set. Class: keep (`tests/feature_ci_coverage.rs` reads the `[features]` table; an empty entry is harmless).
- `docker/cross-musl/Dockerfile`, `perf/scanner_bench_baseline.toml`, `packaging/npm/*`, `fuzz/fuzz_targets/*`, `benches/*`, `src/bin/*`: all referenced (Cross.toml, benchcheck, release workflow, fuzz Cargo.toml, bench entries, `justfile`). Class: keep.

## Counts

| Class | Count | Items |
| --- | --- | --- |
| Safe to delete | 6 | `PushOutcome::is_accepted`, `set_recovery_decision`, unused dependency `tracing` (aegis-config), unused `aegis-scanner` (aegis-audit dependency, aegis-audit dev-dependency, aegis-policy dependency). |
| Needs a decision | 16 | `TargetAnalysis`, `SourceOrigin::Heredoc`/`Pipe`, `matches_prefix_ending_options`, `prefix_scan`, `matches_tokens`, `validation_load_error`, `analyze_blocklist_rule`, `ConfigSourceMap::for_config`, `merge_layer`, `Unratcheted::reason`, `with_path`, `set_confirmation_decision`, the two `RetentionPolicy` constructors, `from_id`, the language manifest items, `scripts/test-tamper-check.test.sh`. |
| Keep | 22 | Each entry marked "keep" above, with its reason. |

Where several items share one finding (for example the two retention constructors), the group counts once. The per-crate lists are authoritative.
