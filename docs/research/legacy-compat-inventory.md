# Legacy compatibility layers inventory

Research for wayfinder ticket #593 (map #591). Static search only; no code was built or changed. `landing/` is excluded. `file:line` references are against commit 06b234d.

## Summary

35 findings. Class counts:

| Class | Count |
| --- | --- |
| keep | 17 |
| needs a decision | 15 |
| safe to delete | 3 |

User-facing versus internal: 32 user-facing (removing them changes how an existing `.aegis.toml`, audit log, snapshot id, hook install, installer, or CLI call behaves), 3 internal only (T2, R5, R6).

Findings by crate: aegis-config 9, aegis-types 3, aegis-audit 7, aegis-snapshot 4, aegis-explanation 1, `src/` 6, `scripts/` and generated files 5.

Not legacy layers, found by the search and left out: `src/shell_compat.rs` (POSIX `$SHELL -c` invocation, the product contract), `trusted_aliases` (a current config feature), `MIN_SUPPORTED_LANGUAGE_ABI` and the "ABI-compatible" grammar checks (tree-sitter pinning), and the `alias` handling in the router (shell `alias` analysis).

Open issue #502 (report a legacy `allowlist = [...]` entry in `aegis config validate` and `aegis status`, read-only) targets finding C4. Its premise is that the legacy string array stays a recognised shape. Do not delete C4 or C1 to C3 before #502 and the 1.0 decision on them land. Issue #249 (closed) removed the allowlist auto-migration in commit c34db3e (#589); the string-array error path is what remains.

Provenance below comes from `git log -S` (first commit that added the text) and `docs/adr/`. Dates are commit dates.

## aegis-config (`crates/aegis-config`)

Paths in this section are relative to `crates/aegis-config/` unless they start with `src/` of the root crate and say so.

- C1. `src/model.rs:273`, `src/model/partial.rs:101`: `alias = "allowlist"` accepts `[[allowlist]]` tables as `[[allow]]`. User-facing. Added in d20fae0 (2026-05-27, allowlist merges into the unified rule system). Documented in `PRD.md:177`, `docs/config-schema.md:124` and pinned by `tests/config_schema_docs.rs:37`. Class: needs a decision (the normative PRD promises it).
- C2. `src/model.rs:281`, `src/model/partial.rs:105`: `alias = "blocklist"` accepts `[[blocklist]]` as `[[block]]`. User-facing. Same origin as C1. Class: needs a decision (decide together with C1).
- C3. `src/amend.rs:69-108`: when the existing file uses a `[[allowlist]]`/`[[blocklist]]` header, amend appends the same legacy header instead of the canonical one. User-facing (changes the text written into a user's file). Added with the alias work (see C1). Class: needs a decision (goes with C1/C2).
- C4. `src/model/serde_helpers.rs:13-28`: `AllowlistField::Legacy(Vec<String>)` recognises an old `allowlist = ["..."]` string array only to reject it with "replace legacy string array with [[allow]] tables". User-facing. Introduced in 5424028 (2026-04-12, installer and versioned config migration); the migration itself was removed in c34db3e (#589, issue #249). Class: needs a decision (#502 will build its read-only diagnostic on this shape; keep until #502 lands).
- C5. `src/model.rs:197-206`: `sandbox.enabled` and `sandbox.required` kept as ignored, `skip_serializing`, schema-`deprecated` fields "for schema v1 compatibility". User-facing: with `deny_unknown_fields`, removing them makes every existing config that sets them fail to load. Added in 345da9a (#583, ADR-029, mandatory sandbox). Class: keep (removal is a config-format break; needs its own decision per the safety contract of #591).
- C6. `src/model/ratchet/sandbox.rs:119,122`, `src/model/ratchet/warning.rs:24-104`, `src/model/merge.rs:9-50`, `src/model.rs:529-538`, `src/validate.rs:214-243`, and in the root crate `src/toggle.rs:41-101`: the `deprecated_sandbox_field` warning pipeline that reports C5. User-facing (warning text in `aegis config validate` and `aegis status`). Added in 345da9a (#583). Class: keep (lives and dies with C5).
- C7. `src/model/serde_helpers.rs:31-72`, `src/model/partial.rs:94`: `config_version` handling. Omitting it defaults to the current version; a value below 1 or above the current is rejected with upgrade or `aegis config init` guidance. User-facing. Introduced in 5424028. Class: keep (a schema version gate is a mechanism, not a shim; only version 1 exists).
- C8. `src/model/template.rs:11`: init template comment "Omit only when loading a pre-version legacy config for migration." No migration exists any more (c34db3e). User-facing text in `aegis config init` output. Class: safe to delete (reword the comment; behaviour of omission stays per C7).
- C9. `src/validate.rs:152-165`: fails closed when `<home>/.aegis/policy.star` exists. User-facing. Added in 73674bd (#577, ADR-028, Starlark DSL removed). Class: keep (security: a silently ignored policy file would weaken posture).

## aegis-types (`crates/aegis-types`)

- T1. `src/sandbox.rs:22`: `#[serde(alias = "NotConfigured")]` next to `rename_all = "snake_case"`, so PascalCase is also read. User-facing only for audit lines written in the one-day window between dbad3b4 (2026-06-15, enum added) and 11bfecb (2026-06-16, alias added). Class: needs a decision (probably safe to delete if no release shipped in that window; check release tags).
- T2. `src/sandbox.rs:34`, `src/sandbox.rs:43`: `as_legacy_active()` and `From<Option<bool>> for SandboxStatus`, the bool/status projections used by the audit writer and reader (see A2, A3). Internal API; its effect is user-facing only through A2 and A3. Added in dbad3b4. Class: keep (needed while A2 and A3 stay).
- T3. `src/assessment.rs:169,187,241`: `DecisionSource`, "retained for v1 compatibility projection" next to `AssessmentBasis`. Still used in live output: root `src/shell_wrapper.rs:136`, `src/policy_output.rs:180` (JSON `decision_source` string), `crates/aegis-tui/src/shared.rs:196`, `crates/aegis-explanation/src/lib.rs:32`. Introduced in 01f5a04 (crate extraction); v2 basis from ADR-022 section 10. Class: needs a decision (user-facing via policy JSON and the audit explanation; the doc comment is stale because it is not only a projection).

## aegis-audit (`crates/aegis-audit`)

- A1. `src/logger.rs:29-110`: `AuditTimestamp` deserialiser accepts Unix seconds (i64 and u64) as well as RFC 3339. User-facing (old `audit.jsonl` lines). Introduced in 3b65754 (2026-03-22). Class: keep (the audit log is append-only, ADR-004; old lines must stay readable and verifiable).
- A2. `src/logger.rs:287-293,405`: writer still emits the legacy `sandbox_active` boolean next to `sandbox_status` "so older readers keep working". User-facing (audit JSON shape). Added in dbad3b4. Class: needs a decision (removing the write changes the log format; it is deliberately outside the integrity hash, see A5).
- A3. `src/logger.rs:352-354`: reader falls back to `sandbox_active` when `sandbox_status` is absent. User-facing. Added in dbad3b4. Class: keep (append-only history).
- A4. `src/logger.rs:127,167-194,517-548`, `src/ledger/commands.rs:35`: optional v1-era fields (`sequence`, ADR-016 axes, `basis`, `analysis`, `snapshot_attempts`, `cwd`, provider fields) deserialised as `None` on old lines. User-facing. ADR-016, ADR-022 section 10, ADR-031. Class: keep (append-only; `None` versus `false` is meaningful).
- A5. `src/logger/integrity.rs:53-60,89-91`: hash payload excludes `sandbox_status` and `sandbox_active` so old chains verify; v1 entries hash byte-for-byte as before. User-facing (chain verification). 0c2fbc2 (2026-04-11) and later. Class: keep (ADR-017; threat-model limitation already recorded).
- A6. `src/logger/writer.rs:179`, `src/logger/query.rs:246`: `normalize_legacy_fields` backfills `pattern_ids` and `allowlist_matched`/`allowlist_effective` on old entries. User-facing (query and export of old lines). Introduced in 0c2fbc2 and moved in 0778c25. Class: keep.
- A7. `src/ledger.rs:24,67-113,185`: Snapshot ledger honours prune entries written without a `snapshots` list, by parsing `aegis prune <id>` out of the command text. User-facing (old logs decide what is rollbackable). Prune command from 11bfecb (2026-06-16); ledger code from 63399ee (#527). Class: keep (ADR-049; wrongly resurrecting a pruned snapshot is worse than the code).

## aegis-snapshot (`crates/aegis-snapshot`)

- S1. `src/snapshot_id.rs:3-22,137-147`: `LEGACY_SEP` (tab), `V2_VERSION`, `is_v2`, `split_v2`. Reads ids written before `v3`. User-facing (rollback of an already recorded snapshot). v3 writer introduced in c02bf37 (#286, #570); the module doc says old ids "stay readable forever" (ADR-026 section 2 owns the codec). Class: keep.
- S2. `src/sqlite.rs:109-125`, `src/mysql/mod.rs:177-204`, `src/postgres/mod.rs:164-191`: per-provider `parse_legacy_snapshot_id` and v2 branches. User-facing, same as S1. Origin 25900e1 (crate extraction); codec rework c02bf37. Class: keep.
- S3. `src/git/mod.rs:114-124`: git id parser accepts `<raw cwd><TAB><hash>`. User-facing, same as S1. Class: keep.
- S4. `src/supabase/mod.rs:26,228-235`: `LEGACY_SNAPSHOT_ID_VERSION = "supabase-v1"` tab ids. User-facing, same as S1. Class: keep.

## aegis-explanation (`crates/aegis-explanation`)

- E1. `src/lib.rs:29-38`: `ScanExplanation.decision_source` documented as the "v1 compatibility projection"; `basis` is `#[serde(skip)]` so v1 audit JSONL is unchanged. User-facing (persisted audit explanation). Class: needs a decision (same decision as T3; do not touch alone).

## Root crate (`src/`)

- R1. `main.rs:81`: `#[command(alias = "install")]` on `install-hooks`. User-facing (CLI call). Added in bb22f62 (2026-04-22, binary-first hook install). Docs and `scripts/agent-setup.sh` still reference it. Class: needs a decision (CLI break).
- R2. `install/mod.rs:164-166`: comment and default "legacy `aegis install` behavior installs both agents" (no target flag means all). User-facing (flag-less call). Class: keep (the behaviour is the current default; only the word "legacy" is stale).
- R3. `install/mod.rs:248-266`, `install/claude.rs:209-215`, `install/uninstall.rs:19`: recognises and prunes old Claude hook registrations (`aegis hook` bare command, `aegis-rewrite.sh`, stale `aegis-pre-tool-use.sh`). User-facing (reinstall and uninstall edit the user's `~/.claude/settings.json`). `aegis-rewrite.sh` from a2c9d04 (2026-04-17); migration from da28103 (2026-06-24, ADR-012); ownership across matchers from 19833f5 (#176). Class: needs a decision (upgrade path from pre-June installs; safe once such installs no longer matter).
- R4. `install/codex.rs:159-167`: removes the old `features.codex_hooks` key on install. User-facing (edits `~/.codex/config.toml`). Added in d6a2c04 (2026-04-21), reshaped in bb22f62. Class: needs a decision.
- R5. `analysis/orchestrate.rs:105-125`, re-export `analysis/mod.rs:43`: `run()` is "the compatibility entrypoint" that wraps `run_with_budget`. Internal: the runtime calls `run_with_budget_in_cwd` (`runtime/context.rs:327`); the only other caller of `run` found is `tests/analysis_orchestrate_runtime.rs:184` (other tests call `run_with_budget*`; not exhaustively checked). Introduced in 44d5897 (#152). Class: safe to delete (move the test to `run_with_budget`; delete a test only with the code it covers).
- R6. `install/hook.rs:230`, `643`: comments that a top-level legacy `decision` field is deliberately not emitted. Internal, not a layer. Class: safe to delete (history-style comment under the #591 comment rule; keep the behaviour and its test).

## Scripts and generated files

- X1. `scripts/agent-setup.sh:1-3`: "compatibility wrapper for `aegis install-hooks`", exercised by `tests/agent_hooks_install.rs`. User-facing (published script). Added in d33509f (2026-04-18). Class: needs a decision.
- X2. `scripts/install.sh:502`: rejects `AEGIS_SETUP_MODE` and `AEGIS_SKIP_SHELL_SETUP` with a "deprecated" failure. User-facing (installer exit). Added in 27b36d4 (2026-04-20). Class: needs a decision (fail-hard tombstone; the error stays useful for old CI scripts).
- X3. `scripts/install.sh:544`: tolerates a missing THIRD_PARTY_NOTICES asset for "pre-v0.6.3" releases. User-facing (installing an old pinned version). Class: needs a decision.
- X4. `scripts/uninstall.sh:291-297`: removes the `aegis-rewrite.sh` payload and its registration. User-facing. Same origin as R3. Class: needs a decision (goes with R3).
- X5. `aegis-schema.json:757-765`: generated `deprecated: true` entries for C5. User-facing. Class: keep (generated from C5; never edit by hand).

## Notes

- The audit-log and snapshot-id layers (A1 to A7, S1 to S4) are the clearest "keep": ADR-004 makes the audit log append-only, and removing a reader would strand recorded rollback ids.
- No Aegis bypass (a dangerous command wrongly allowed) was found during this search.
