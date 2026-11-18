# Workarounds and duplicated helpers inventory

Research for wayfinder map #591, ticket #594. Static search of `src/`, `crates/*`, `tests/`, `scripts/`, `.github/`, `fuzz/` and `benches/` at commit 06b234d. `landing/` and the vendored bubblewrap C sources are excluded. Nothing was built or run, so every "stale" or "dead" claim below is read from the source and should be confirmed before deleting.

## Summary

- Explicit markers: one `shortcut:` comment (`aegis-snapshot`) and no TODO, FIXME, HACK or XXX markers in code. The `XXXX` hits are magic-byte test fixtures; the `TODO` hits are test inputs such as `grep -r TODO src/`.
- The code documents workarounds as prose ("known gap", "accepted limitation", "boundary-forced duplication"), not as markers. The findings below come from reading those, plus a scan of function names that occur in more than one file.
- Counts are given below.
- Biggest clusters: the mysql, postgres and docker snapshot plugins; the Python and JavaScript-family adapters in `aegis-language`; six copies of the `HOME` lookup; and test-binary and git helpers repeated across `tests/`.

Classes: SD = safe to delete, ND = needs a decision, K = keep (the reason follows).

Findings: 89 entries. Safe to delete: 6. Needs a decision: 39. Keep: 44. Several entries stand for a group of copies; the ten duplicate groups are detailed in the Duplicates section.

## Per crate

### src/ (root `aegis` crate)

- `src/analysis/router/tests/unclaimed_launcher_operands.rs:331` K. "Known gap": a slash-less dynamic launcher operand cannot be told from data without changing ordinary commands. Pinned as accepted behaviour.
- `src/analysis/mod.rs:14` ND. `sha256_hex(bytes)` formats a digest by hand; `aegis-snapshot` has a different `sha256_hex(path)` (D10).
- `src/analysis/orchestrate.rs:375` ND. `home_dir_optional`, one of six HOME lookups (D3).
- `src/toggle.rs:128` ND. `home_dir`, wraps `home_dir_optional` into an error (D3).
- `src/toggle.rs:140` ND. `home_dir_optional`, identical to the orchestrate copy (D3).
- `src/install/mod.rs:480` ND. `home_dir`, checks HOME then USERPROFILE with different empty-value handling (D3).
- `src/update/state.rs:82` ND. `home_dir`, same lookup with `UpdateError::NoHome` (D3).
- `src/runtime/context.rs:759` ND. `policy_star_path()` rebuilds `$HOME/.aegis/policy.star`; `aegis-config/src/validate.rs:149` builds the same path from a passed-in home (D10).
- `src/runtime/context.rs:479` K. `git_stage_report` forwards to the scanner; the runtime context stays the single facade callers use.
- `src/execution/mod.rs:408` ND. `sandbox_status_before_preparation(Option<&SandboxConfig>)`; same decision as `src/watch/sandbox.rs:259`, which takes the runtime context (D10).
- `src/watch/sandbox.rs:259` ND. Second copy of the `NotAttempted`/`NotConfigured` decision (D10).
- `src/execution/mod.rs:183` K. `#[expect(too_many_arguments)]` with a stated reason (mirrors the Wrapper execution seam).
- `src/analysis/recursive.rs:52` K. `#[allow(clippy::too_many_arguments)]` on a recursive walker; a struct would only move the arguments.
- `src/analysis/router/segments/unclaimed.rs:92` K. `basename` copy; boundary-forced (D5).
- `src/analysis/router.rs:528` K. `inline_body` copy of the one in `aegis-language`; boundary-forced (D5).
- `src/install/mod.rs:24` ND. `shell_quote`; `tests/support/agent_hooks.rs:103` defines it again (D9).
- `src/install/hook.rs:230` K. Comment explains why only the `permissionDecision` form is emitted; describes a deliberate absence of the legacy field.
- `src/install/claude.rs:209` K. Prune-then-add removes legacy bare `aegis hook` and `aegis-rewrite.sh` registrations; still needed for upgraders (pinned in `src/install/claude/tests.rs:455`).
- `src/install/codex.rs:159` K. Removes the legacy `codex_hooks` feature flag on install; migration for old config files.
- `src/rollback_last/tests.rs:130` K. Legacy-id fixture; ids without a cwd exist in real logs.
- `tests/contracts_docs.rs:281` ND. A test its own doc comment calls temporary: delete once #229 and #230 make the Sandbox mandatory. Check whether those issues are closed.
- `tests/support/mod.rs:13` K. Crate-level `#![allow(dead_code)]`; each integration test binary uses a different subset of the support module, so the lint is wrong by construction. Same at `tests/support/installer.rs:10`.

### aegis-snapshot

- `crates/aegis-snapshot/src/readiness.rs:54` ND. The only `shortcut:` comment: `NotSupported` stands in for "no artifact to parse"; upgrade when the audit schema gains a dedicated value. Touches the audit schema, so it is a decision.
- `crates/aegis-snapshot/src/mysql/mod.rs:21` ND. `EXECUTABLE_BUSY_ERRNO = 26`, third copy with `postgres/mod.rs:19` and `docker/mod.rs:22` (D1).
- `crates/aegis-snapshot/src/mysql/mod.rs:160` ND. `is_executable_busy`; copies at `postgres/mod.rs:147` and `docker/mod.rs:113` (D1).
- `crates/aegis-snapshot/src/mysql/mod.rs:55` ND. Plugin skeleton nearly identical to `postgres/mod.rs:55` (D1).
- `crates/aegis-snapshot/src/mysql/mod.rs:202` K. `parse_legacy_snapshot_id`; also `postgres/mod.rs:189`, `sqlite.rs:119`, `git/mod.rs:114`. Existing audit logs hold legacy ids, so rollback must still read them; tests pin each.
- `crates/aegis-snapshot/src/secure_fs.rs:83` ND. `effective_uid` and `insecure` (`:213`) duplicated in `aegis-audit` (D2).
- `crates/aegis-snapshot/src/paths.rs:24` ND. `home_dir()` that does not filter an empty HOME (D3).
- `crates/aegis-snapshot/src/registry.rs:513` K. `record_debug`/`record_str` in a test-only tracing visitor; `src/diagnostics.rs:209` is a different visitor (truncation). Same name, different job.
- `crates/aegis-snapshot/src/docker/mod.rs:117` ND. Busy-retry delay for ETXTBSY (#436) is compiled into production paths; decide whether it is a test-only workaround that should live in test support or a real production guard.
- `crates/aegis-snapshot/src/test_support.rs:11` K. Renames a freshly written mock binary to avoid ETXTBSY (#436); test-only workaround for kernel behaviour.

### aegis-sandbox

- `crates/aegis-sandbox/src/lib.rs:146` ND. `SandboxExecutor` and its `run()` path (`src/linux.rs:19`, `src/macos.rs:26`) are called "legacy" in the module doc. No caller in `src/` uses them; only the crate's own tests (`src/lib.rs:544`, `src/support.rs:173`). Candidate for removal.
- `crates/aegis-sandbox/src/lib.rs:399` ND. Wrapper of `apparmor_userns_restricted`, also defined at `src/bwrap.rs:288`; confirm it is not dead on non-Linux.
- `crates/aegis-sandbox/src/lib.rs:471` K. `run_inner_landlock_wrapper` and `landlock_abi` (`:477`) are pass-throughs that keep the `landlock` module private.
- `crates/aegis-sandbox/src/lib.rs:104` K. `WriteRoot` mirror of the config type; the crate may not depend on `aegis-config`.
- `crates/aegis-sandbox/src/bwrap.rs:380` K. Script passed by fd to avoid ETXTBSY (#513); documented workaround for a real race.
- `crates/aegis-sandbox/src/macos.rs:99` K. Mirrors the bwrap symlink-confusion guard; platform split with a different mechanism.

### aegis-language

- `crates/aegis-language/src/languages/bash.rs:102` K. Refuses non-ASCII source because tree-sitter-bash 0.25.1's scanner crashes on it. Workaround for an upstream bug; remove when the grammar pin moves past it. Worth a tracking issue.
- `crates/aegis-language/src/languages/python.rs:93` ND. `classify_path` and six more helpers (`exec_language`, `first_positional_arg`, `operand_certainty`, `is_string_literal`, `string_literal_content`, `span_for`) repeat in `family.rs` (D4).
- `crates/aegis-language/src/languages/family.rs:68` ND. The other side of D4.
- `crates/aegis-language/src/languages/bash_word.rs:131` K. Second copy of the `aegis-parser` tokenizer word rules, forced by the leaf-crate rule (ADR-022 §8); guarded by `tests/bash_word_parity.rs` (D6).
- `crates/aegis-language/src/languages/bash.rs:233` K. `is_python` is intentionally broader than the router's list; the doc says changes must be mirrored by hand, so drift is a risk.
- `crates/aegis-language/src/operation.rs:14` K. Operation vocabulary mirrors `aegis-types` one for one; boundary-forced, pinned by a conversion test.
- `crates/aegis-language/src/router.rs:125` K. `inline_body`, `basename` (`:141`), `shell_words` (`:153`) copy root router helpers (D5).
- `crates/aegis-language/src/worker.rs:349` K. `XXXX` is a bad-magic fixture, not a marker.

### aegis-parser

- `crates/aegis-parser/src/lib.rs:151` K. `ENV_SPLIT_MAX_DEPTH = 8` equals the router `MAX_WRAP_DEPTH` and the queue depth ceiling in value; documented dependency-DAG duplication. Three constants that must move together.
- `crates/aegis-parser/src/tokenizer.rs:169` K. Original of the `bash_word.rs` copies (D6), with `tokenizer/ansi_c.rs:28`.
- `crates/aegis-parser/src/prefix_match.rs:170` ND. Test helpers `single`, `alts` repeated in four places (D7).
- `crates/aegis-parser/src/tests/parsing_tests.rs:284` ND. Comment says the pipeline path uses `split_top_level_command_groups`, "the second copy" of the logic; verify the second copy and whether both are still needed.

### aegis-scanner

- `crates/aegis-scanner/src/scanner/tests/h3_followups.rs:76` SD. Comment says the test "will be RED until the flag is added to the FS-015 alt list". The flag is there (`patterns/builtins_a.rs:122`) and the test is not ignored. Stale comment.
- `crates/aegis-scanner/src/scanner/tests/h3_gaps.rs:256` ND. Same "will be RED" wording for DB-006 `redis-cli GET FLUSHALL`; the rule at `patterns/builtins_a.rs:437` still uses `any_star`. Run the test: either the comment is stale or something else narrows the match.
- `crates/aegis-scanner/src/patterns/builtins_a.rs:18` K. FS-011 has a matcher-side predicate for bundled wipefs flags, "intentionally not a generic prefix-rule feature". Documented special case.
- `crates/aegis-scanner/src/patterns.rs:169` K. Duplicate prefix-rule ids are intentional (one logical rule, several delivery forms).
- `crates/aegis-scanner/src/scanner/effect_opaque.rs:23` K. Accepted v1 limitation (ADR-016), fail-safe direction, reasoned in the doc.
- `crates/aegis-scanner/src/scanner/highlighting.rs:52` SD. Test-only public wrapper over a `pub(super)` function, used by one in-crate test; also duplicated by name in `aegis-tui` (D8).
- `crates/aegis-scanner/src/scanner/tests/edge_cases.rs:442` ND. Comment says a pipeline copy of the background-`&` discriminator exists elsewhere; confirm both copies and merge or pin.
- `crates/aegis-scanner/src/scanner/prefix_rule.rs:326` ND. Test helpers `single`, `alts` (D7).

### aegis-tui

- `crates/aegis-tui/src/shared.rs:61` SD. Test-only `sorted_highlight_ranges_for_tests`, a second copy of the scanner's wrapper (D8); `shared.rs:29` `build_highlighted_command` is `#[cfg(test)]` only too.
- `crates/aegis-tui/src/stdout_renderer.rs:26` ND. Non-interactive renderer repeats the confirm and policy-block display that `execution_renderer.rs` also holds, with a different sink.
- `crates/aegis-tui/src/execution_renderer.rs:11` K. `show_*` methods appear in the trait, `TerminalRenderer` and `TestRenderer`; that is the trait shape, not duplication.
- `crates/aegis-tui/tests/api.rs:3` SD. Header says the tests "fail until the contents of `src/ui/confirm/` are extracted"; the crate exists, so the sentence is stale.

### aegis-config

- `crates/aegis-config/src/pattern_match.rs:9` ND. `policy_pattern_matches` is the config-side prefix matcher; parser and scanner hold matchers with the same prefix logic over different token types (D7).
- `crates/aegis-config/src/validate.rs:149` ND. `policy_star_path`, second copy (D10).
- `crates/aegis-config/src/allowlist.rs:85` ND. `global`/`project` constructors (`:93`, `:252`, `:260`), `from_layered_rules` (`:164`, `:306`), `match_reason` (`:189`, `:333`) and "best-effort source location" (`:125`, `:292`) repeat across two near-parallel impl blocks. Decide whether one generic body is worth it.
- `crates/aegis-config/src/allowlist.rs:153` K. Documented legacy constructor shape kept for callers.
- `crates/aegis-config/src/model/ratchet/direction.rs:152` K. `#[allow(dead_code)]` on a field that exists only to document why a field sits outside the ratchet ("Not read at runtime").
- `crates/aegis-config/src/model/ratchet/direction.rs:21` K. `too_many_arguments` allow on a merge helper.
- `crates/aegis-config/src/ceiling/workspace.rs:292` K. `#[allow(unused_mut)]`; the `mut` is needed only in the macOS cfg block.
- `crates/aegis-config/src/model/ratchet/warning.rs:51` K. `cfg_attr(not(test), allow(unused_variables))` for a test-only recording hook.
- `crates/aegis-config/src/model/serde_helpers.rs:25` K. Legacy table names (`allowlist`, `blocklist`) still accepted or rejected with guidance; pinned in `model/tests/deser.rs:31`, `model/tests/migration.rs:241`, `amend.rs:69`.

### aegis-audit

- `crates/aegis-audit/src/secure_fs.rs:299` ND. `effective_uid`, with `insecure` (`:305`); duplicate of the snapshot crate (D2).
- `crates/aegis-audit/src/logger/writer.rs:179` K. `normalize_legacy_fields` backfills v1 lines; a test pins that `append` calls it exactly once.
- `crates/aegis-audit/src/logger/integrity.rs:53` K. Documented gap: `sandbox_status` is not in the hash payload; closing it needs `chain_alg` versioning. Tracked in `docs/threat-model.md`. Worth an issue if none exists.
- `crates/aegis-audit/src/logger.rs:190` SD. Doc says `snapshot_attempts` is `None` "while the runtime does not yet populate it"; the runtime writes attempts now (`tests/audit_snapshot_attempts.rs`). Stale sentence.
- `crates/aegis-audit/src/logger.rs:405` K. Legacy `sandbox_active` mirror (with `:352` and `src/ledger.rs:67`, legacy prune ids); needed to read old logs.
- `crates/aegis-audit/src/logger/tests/integrity_tests.rs:23` SD. `append_normalizes_legacy_fields_only_once` is defined with the same body in `append_tests.rs:128`.

### aegis-types

- `crates/aegis-types/src/analysis.rs:15` ND. Module doc says `Assessment` and `MatchResult` "are not yet refactored onto this model (that is a later slice)". Either the slice is done and the sentence is stale, or it is still planned.
- `crates/aegis-types/src/sandbox.rs:34` K. `as_legacy_active` mirror for the old boolean field.
- `crates/aegis-types/src/decision.rs:41` K. `from_str` and its rejection test repeat in `risk.rs:45`; two small enums, a macro would be larger than the code.

### aegis-explanation, aegis-policy

- `crates/aegis-explanation/src/lib.rs:122` K. Field omitted when false so legacy lines hash the same.
- `crates/aegis-policy/src/engine.rs:285` K. "Best-effort" Danger snapshot is specified behaviour, not a workaround.

### tests/, scripts/, CI

- `tests/cli_integration.rs:8` ND. `aegis_bin()` and `base_command()` are redefined in about 20 test files although `tests/support/mod.rs:26` exports both (D9).
- `tests/snapshot_prune_git.rs:29` ND. `init_git_repo` repeated in 7 files and `run_git` in 2, with different argument order; some strip `GIT_DIR` and these do not (D9).
- `tests/lint_gate_ci.rs:28` ND. `repo_path`, `repo_file`, `read_repo_file`, `read_file` defined 12 or more times; `tests/common/mod.rs:14` has `repo_root` (D9).
- `tests/npm_live.rs:26` ND. `version_matches` and its test repeat in `tests/homebrew_live.rs:31`.
- `tests/analysis_orchestrate.rs:15` ND. `safe_baseline` in 5 files, `dangerous_recursive_delete` in 2 (D9).
- `scripts/hooks/claude-code.sh:11` ND. Comment calls a unified hook template "a tracked follow-up (ADR-012)"; the Claude and Codex shims differ by 28 diff lines, the two session-start scripts by 21.
- `plugins/aegis/hooks/aegis-pre-tool-use.sh:78` K. Copy of `scripts/hooks/claude-code.sh` differing only in `AEGIS_BIN=aegis` against `__AEGIS_BIN__`; same for `aegis-session-start.sh:104`. I did not confirm a CI check pins this.

## Duplicates

D1. mysql, postgres, docker snapshot plugins. `mysql/mod.rs:55-150` against `postgres/mod.rs:55-150` define the same set: `build_common_args`, `reserve_dump_path`, `dump_path_candidate`, `sanitized_database_label`, a `*_with_busy_retry` wrapper, `build_snapshot_id`, `parse_snapshot_id`, `parse_legacy_snapshot_id`, `is_executable_busy` and `EXECUTABLE_BUSY_ERRNO`. They differ in binary names (`mysqldump`/`mysql` against `pg_dump`/`pg_restore`), flag style (`--host=` against `-h`), file suffix (`.sql` against `.dump`), `spawn()` against `output().await` in the retry wrapper, and mysql alone has `validate_dump_file_name`. `docker/mod.rs` has its own `EXECUTABLE_BUSY_ERRNO` and `is_executable_busy`. Cheapest first step: one shared constant and predicate.

D2. secure_fs. `aegis-snapshot/src/secure_fs.rs` and `aegis-audit/src/secure_fs.rs` each define `effective_uid` and `insecure`. The snapshot copy has a test injection seam (`INJECTED_EFFECTIVE_UID`), takes a `plugin` argument and returns `SnapshotError`; the audit copy has no seam and returns `AuditError`. Both crates are leaves, so sharing needs `aegis-types` or a new crate.

D3. HOME lookup, six copies. `src/toggle.rs:140` and `src/analysis/orchestrate.rs:375` are identical (HOME, then USERPROFILE, drop empty). `src/update/state.rs:82` is the same with `UpdateError::NoHome`. `src/install/mod.rs:480` filters empty values on each variable in turn, so an empty HOME falls through to USERPROFILE, unlike the first three. `crates/aegis-snapshot/src/paths.rs:24` never filters empty values. `src/toggle.rs:128` wraps one of them in an error. An empty `HOME` gives different answers per copy.

D4. Python against JavaScript family. `python.rs` and `family.rs` both define `classify_path`, `exec_language`, `first_positional_arg`, `operand_certainty`, `is_string_literal`, `string_literal_content`, `span_for` and the interpret loop. They differ in node kinds (`keyword_argument`, `concatenated_string`, `interpolation`), the API table, and the `ExecLang` enum (Python or Bash against Js or Bash). `span_for` and `exec_language` are likely identical apart from the enum; I did not diff them line by line.

D5. Router helpers. `inline_body` in `src/analysis/router.rs:528` takes `&[&str]`; the copy in `aegis-language/src/router.rs:125` takes `&[String]`; the bodies match except one comment. `basename` uses `rsplit('/')` in the root and `rsplit_once('/')` in the language crate, with the same result. Boundary-forced.

D6. Bash word rules. `aegis-parser/src/tokenizer.rs` and `tokenizer/ansi_c.rs` against `aegis-language/src/languages/bash_word.rs`: `ifs_marker_len` (different code, same job), `read_ansi_c_quoted`, `read_escape`, `read_hex`, `read_control`. `tests/bash_word_parity.rs` is the only guard against drift.

D7. Prefix matchers. `aegis-config/src/pattern_match.rs` (`policy_pattern_matches`), `aegis-parser/src/prefix_match.rs` (`matches_prefix`) and `aegis-scanner/src/scanner/prefix_rule.rs` share prefix logic over different token types (`PolicyPatternToken` with owned strings; `PatternToken` with borrowed strings, `ShortFlag` and any-star). The test helpers `single`, `alts` and `empty_pattern_never_matches` appear in all of them and in `src/planning/policy_rules.rs:86`.

D8. Highlight test wrapper. `aegis-scanner/src/scanner/highlighting.rs:52` and `aegis-tui/src/shared.rs:61` both define `sorted_highlight_ranges_for_tests`. The scanner one calls `sorted_highlight_ranges`; the TUI one is the helper behind the `#[cfg(test)]` `build_highlighted_command`.

D9. Test support. `aegis_bin` and `base_command` (about 20 files), `read_audit_entries` (8), `init_git_repo` (7), `run_git` (2), the `repo_path` family (12 or more), `run_snapshot_list` (3), `seed_snapshot` (2), `run_prune` (2), `stash_hashes`, `stash_list`, `create_stash` (2 each), `aegis_test_binary` (2), `shell_quote`, and a test `sha256_hex` (`tests/support/installer.rs:81`). Most copies are byte-identical or differ in argument order (`run_git(args, cwd)` against `run_git(repo, args)`). Isolation differs: only some strip `GIT_DIR`, and a leaked `GIT_DIR` from a linked-worktree hook points tests at the real repo.

D10. Single pairs.
- `sandbox_status_before_preparation`: `src/execution/mod.rs:408` takes `Option<&SandboxConfig>`; `src/watch/sandbox.rs:259` takes the runtime context and checks `config().sandbox.is_some()`. Same result.
- `policy_star_path`: `src/runtime/context.rs:759` reads `$HOME`; `aegis-config/src/validate.rs:149` takes a home path.
- `sha256_hex`: `src/analysis/mod.rs:14` formats bytes; `aegis-snapshot/src/supabase/mod.rs:402` hashes a file. Different inputs; only the hex formatting overlaps.
- Same name, different job (not duplicates): `record_debug`/`record_str` (two tracing visitors), `lock_path` (update state against audit writer), `shell_words` (quote-aware splitter in `aegis-language/src/router.rs:153` against an identifier splitter in `aegis-parser/src/heredoc_data_consumer.rs:358`), `format_snapshot_listing` (`src/cli_commands.rs:488`, the one at `:676` is a test).
