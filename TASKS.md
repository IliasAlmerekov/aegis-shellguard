# TASKS — Historical registry of Aegis security findings

> Sources: the 2026-06-23 reviewer security audit and the 2026-06-24 live
> crash-test of `aegis 0.5.9`.

**This file is a registry, not a gate.** It records which findings those audits
raised, what each one is, and where the work on it lives. It carries no status,
no acceptance criteria, and no ordering — each of those now has exactly one home
elsewhere:

- **What blocks 1.0** is the
  [`1.0` milestone](https://github.com/IliasAlmerekov/aegis-shellguard/milestone/1)
  and nothing else
  ([ADR-027](docs/adr/adr-027-one-1-0-release-gate-lives-in-the-issue-tracker.md)).
  A finding blocks 1.0 if and only if it falsifies something `PRD.md` promises,
  so the severity prefix carries no release weight
  ([#202](https://github.com/IliasAlmerekov/aegis-shellguard/issues/202)).
- **Whether a finding is still open** — and what remains of it — is the state of
  the issue linked from it. A finding with no issue was closed before the tracker
  became the gate; its evidence is the traceability line, `CHANGELOG.md`, and git.
- **Implementation order** is expressed as native blocked-by relationships
  between the issues in the milestone, never as prose here.
- **Acceptance criteria** live on the issue, and the architectural rationale in
  `docs/adr/`. Where a finding predates its issue, the criteria it was closed
  against are in git history and in the linked plan under `docs/plans/`.

## ID vocabulary

Finding IDs in this file are the **only** meaning of these prefixes anywhere in the
repository. The letter is severity: `C<n>` critical (P0), `H<n>` high (P1), `M<n>`
medium (P2), `P3-<n>` low/informational. A trailing letter splits one finding into
independently closable parts (`H7a`/`H7b`); a trailing `.<n>` names an implementation
slice of one finding (`M5.3` is the third slice of `M5`, not a separate finding).

Severity is the reviewer's original judgement of impact, preserved as written. It
is not a release verdict.

Roadmap milestones use a disjoint namespace — `Phase <n>` and `L<n>` in `ROADMAP.md` —
and never `C`/`H`/`M`/`P3`. See `CONVENTION.md` §11.

---

## P0 — Critical

### C1 — Uppercase bypasses built-in regex patterns

- **Finding:** destructive commands written in uppercase bypassed regex
  verification after the case-insensitive quick pass.
- **Traceability:** commits `60de12d`, `4d8d58b`; scanner mixed-case regression
  tests.

### C2 — Literal `$IFS` command obfuscation bypasses classification

- **Finding:** unquoted literal `$IFS` / `${IFS}` can act as shell separators
  while remaining fused inside scanner tokens.
- **Traceability:** commit `a920370`; parser and scanner `$IFS` regressions.

### C3 — Project config can weaken trusted security settings

- **Finding:** project-local `.aegis.toml` could weaken mode, recovery,
  confinement, provider targets, or policy outcomes inherited from trusted
  config.
- **Traceability:** [ADR-013](docs/adr/adr-013-project-config-security-ratchet.md);
  commits `86f38ad`, `f4bd0a7`, `c834477`, `5e6ab59`; config ratchet tests.

### C3-residual — Project rules and audit integrity escaped the first ratchet

- **Finding:** project `[[rules]] Allow` and `audit.integrity_mode = "Off"`
  remained last-wins after the initial C3 fix.
- **Traceability:** [ADR-013](docs/adr/adr-013-project-config-security-ratchet.md);
  commit `5e6ab59`; `c3_residual` and policy-planning regressions.

### C4 — Launcher and absolute-path prefixes bypass token-prefix rules

- **Finding:** token-prefix lookup used the literal first token, so absolute
  paths and launcher prefixes such as `rtk`, `sudo`, or `env` hid the effective
  program.
- **Traceability:** [ADR-014](docs/adr/adr-014-launcher-and-absolute-path-normalization-for-token-prefix-detection.md);
  commit `bdfbaf9`; prefix-normalization regressions.

---

## P1 — High

### H1 — Standalone `&` is not a command separator

- **Finding:** background-separated commands could remain one scan target and
  hide a destructive effective program.
- **Traceability:** commit `54743de`; parser and scanner ampersand regressions.

### H2 — Destructive SQL inside database CLI arguments is missed

- **Finding:** destructive SQL delivered through `psql -c`, `mysql -e`, wrappers,
  or compound forms did not match first-token SQL rules.
- **Traceability:** [ADR-015](docs/adr/adr-015-destructive-sql-detected-by-regex-not-token-prefix.md);
  commit `106ac04`; destructive-SQL delivery regressions.

### H3 — High-impact destructive command families are missing

- **Finding:** destructive filesystem and cloud forms including `wipefs`,
  `unlink`, writes to `authorized_keys`, shell-rc clobbering, S3/gsutil deletion,
  and related sibling commands were unclassified.
- **Traceability:** commits `e2ddd5d`, `796d4a0`; scanner `h3_gaps` and built-in
  example tests.

### H4 — Agent hooks can fail open when Aegis is unavailable

- **Finding:** a missing `aegis` binary could make a managed shell hook exit
  without a deny response.
- **Traceability:** [ADR-007](docs/adr/adr-007-shell-hooks-share-one-managed-helper-but-must-not-fail-open.md);
  commit `9667a02`; `tests/agent_hooks.rs`.

### H5 — Audit hash-chain claims exceed the integrity contract

- **Finding:** an unkeyed, locally stored SHA-256 chain detects accidental
  corruption and some edits, but cannot prove adversarial tamper-evidence against
  an actor who can rewrite or truncate the whole log.
- **Traceability:** [plan](docs/plans/2026-07-14-h5-audit-integrity-contract.md);
  [ADR-004](docs/adr/adr-004-snapshots-are-best-effort-audit-is-append-only.md);
  [ADR-017](docs/adr/adr-017-audit-integrity-chain-has-no-external-anchor.md);
  commit `ad9c947` (PR #122).

### H6 — Snapshot paths are not proven contained in the snapshot store

- **Finding:** path validation rejects absolute paths and `..` but does not prove
  that a resolved artifact remains inside the configured snapshot root before
  overwrite or deletion.
- **Traceability:** [plan](docs/plans/2026-07-14-h6-snapshot-path-containment.md);
  [ADR-018](docs/adr/adr-018-snapshot-path-containment.md); commit `e26c7e7`.

### H7a — Snapshot artifacts inherit overly broad permissions

- **Finding:** database dumps and snapshot directories can be created with
  process-umask defaults that expose database contents or credentials to other
  local users.
- **Traceability:** [plan](docs/plans/2026-07-14-h7a-snapshot-artifact-permissions.md);
  [ADR-019](docs/adr/adr-019-owner-only-snapshot-artifact-permissions.md).

### H7b — Audit artifacts follow unsafe paths and inherit broad permissions

- **Finding:** audit log, rotation, and lock-file creation rely on ordinary
  `OpenOptions`/`create_dir_all`, allowing broad modes and symlink-following on
  security-artifact paths.
- **Traceability:** [plan](docs/plans/2026-07-14-h7b-audit-file-hardening.md);
  [ADR-020](docs/adr/adr-020-owner-only-audit-artifacts-and-no-follow-opens.md);
  `crates/aegis-audit/src/secure_fs.rs` and logger regressions
  (`append_creates_owner_only_audit_directories_and_artifacts`,
  `append_rejects_a_symlinked_active_log_without_touching_its_target`,
  `append_rejects_a_symlinked_lock_without_touching_its_target`,
  `append_rejects_a_symlinked_immediate_parent`,
  `rotation_rejects_an_unsafe_managed_slot_before_mutating_archives`,
  `unsafe_staging_aborts_compressed_rotation_before_archive_mutation`);
  non-Unix limits stated in ADR-020;
  [required CI run](https://github.com/IliasAlmerekov/aegis-shellguard/actions/runs/30803123790)
  on [PR #153](https://github.com/IliasAlmerekov/aegis-shellguard/pull/153).

### H8 — Destructive Git forms lack token-prefix coverage

- **Finding:** force-push, forced branch deletion, and stash drop/clear could pass
  without the intended Git rule.
- **Traceability:** commit `b1b64183`; C4 commit `bdfbaf9`; built-in Git rule
  examples and scanner edge-case tests.

### H9 — ADR-016 required recovery can degrade silently

- **Finding:** ADR-016 marks bounded `Effect-opaque execution` and requests a
  recovery backstop, but execution can still proceed when no required snapshot is
  created. This finding does **not** claim that Aegis can classify arbitrary
  dynamic evaluation, encoded payloads, interpreter library calls, or TOCTOU;
  those remain outside the heuristic scanner contract.
- **Traceability:** [plan](docs/plans/2026-07-09-h9-effect-opaque-recovery-backstop.md);
  [ADR-016](docs/adr/adr-016-effect-opaque-execution-uses-recovery-backstops.md);
  iterations 1–3 commit `8dd5392`; [Shell tests](tests/recovery_degradation.rs)
  (`noninteractive_required_recovery_degradation_denies_before_child_execution`,
  `interactive_recovery_deny_prevents_child_execution`,
  `interactive_recovery_run_once_executes_and_records_human_approval`,
  `force_interactive_env_cannot_enable_recovery_override_without_tty`,
  `degraded_audit_write_failure_remains_fail_closed`);
  [Watch tests](tests/watch_mode.rs)
  (`watch_without_tty_denies_required_recovery_degradation_before_execution`,
  asserting the audited `recovery_degradation` reason);
  [docs tests](tests/contracts_docs.rs);
  [required CI run](https://github.com/IliasAlmerekov/aegis-shellguard/actions/runs/30803123790)
  on [PR #153](https://github.com/IliasAlmerekov/aegis-shellguard/pull/153).
  The no-new-risk-level and no-package-runner-expansion constraints hold; the
  Script source inspection and slow-path file reads that L1 adds are a separate,
  ADR-022-authorized stage off the safe hot path, not part of this fix.

---

## P2 — Medium

### M1 — Optional Sandbox degradation is not reliably visible

- **Finding:** when optional execution confinement is configured but unavailable,
  Aegis may continue unconfined with only a tracing warning that the operator
  never sees.
- **Traceability:** [required CI run](https://github.com/IliasAlmerekov/aegis-shellguard/actions/runs/30803123790)
  on [PR #153](https://github.com/IliasAlmerekov/aegis-shellguard/pull/153);
  [plan](docs/plans/2026-07-14-m1-sandbox-degradation-contract.md);
  [ADR-003](docs/adr/adr-003-aegis-is-a-heuristic-guardrail-not-a-sandbox.md);
  [ADR-021](docs/adr/adr-021-sandbox-preparation-reports-the-actual-execution-path.md);
  [Shell lifecycle tests](src/shell_flow/sandbox_lifecycle_tests.rs);
  [Watch lifecycle tests](src/watch/sandbox.rs);
  [async architecture contract](tests/architecture_boundaries.rs);
  [package contract](tests/release_docs.rs);
  [docs contract](tests/contracts_docs.rs).

### M2 — Untrusted custom regexes lack resource limits

- **Finding:** project/user regex compilation has no explicit pattern-length,
  automaton-size, or DFA-size budget.
- **Issue:** [#217](https://github.com/IliasAlmerekov/aegis-shellguard/issues/217)
- **Traceability:** [plan](docs/plans/2026-07-14-m2-custom-regex-limits.md).

### M3a — Disabled Toggle state is operationally invisible

- **Finding:** the intentional global `Toggle` can leave Aegis in unguarded
  passthrough for multiple sessions without a visible indication on shell-wrapper
  and hook surfaces.
- **Traceability:** [plan](docs/plans/2026-07-14-m3a-disabled-toggle-visibility.md);
  [ADR-005](docs/adr/adr-005-global-toggle-at-command-boundaries.md),
  [ADR-006](docs/adr/adr-006-ci-detection-has-an-explicit-override-contract.md),
  [ADR-007](docs/adr/adr-007-shell-hooks-share-one-managed-helper-but-must-not-fail-open.md).
  Implementation: commits `671b261` (session-start visibility), `b90ca2a` (installed
  hook environment isolation), `3646fc7` (session hook JSON protocol, PR #162).
  Closure: `tests/toggle_parity.rs` (notice ↔ `aegis status` agreement across six
  environments), `tests/contracts_docs.rs::m3a_docs_keep_disabled_passthrough_and_hook_refresh_explicit`,
  `tests/agent_hooks.rs` session-start cases, `tests/toggle_cli.rs` status cases plus
  `a_successful_toggle_appends_an_audit_entry_for_each_transition` and the two
  audit-failure cases, and the coexistence and matcher-repair cases in
  `tests/agent_hooks_install.rs` covering the two install defects this closure
  found and fixed.

### M3b — Non-canonical `aegis` hook commands bypass wrapping

- **Finding:** a hook that treats any command beginning with `aegis` as already
  wrapped can be bypassed with a malformed or prefixed command.
- **Traceability:** [ADR-011](docs/adr/adr-011-hooks-rewrite-transparently-in-rust-and-setup-shell-escapes.md);
  commit `091950c`; hook rewrite tests.

### M4 — Hook panics can produce no deny response

- **Finding:** an unwind across the hook entry point can leave the agent without
  a structured deny response.
- **Issue:** [#177](https://github.com/IliasAlmerekov/aegis-shellguard/issues/177),
  split into [#178](https://github.com/IliasAlmerekov/aegis-shellguard/issues/178),
  [#179](https://github.com/IliasAlmerekov/aegis-shellguard/issues/179),
  [#180](https://github.com/IliasAlmerekov/aegis-shellguard/issues/180),
  [#181](https://github.com/IliasAlmerekov/aegis-shellguard/issues/181)
- **Traceability:** [plan](docs/plans/2026-07-14-m4-hook-panic-fail-closed.md);
  [ADR-023](docs/adr/adr-023-hook-panic-fails-closed-in-two-layers.md);
  `src/install/hook.rs`; `tests/agent_hooks_m4.rs`.

### M5 — Remaining point pattern gaps

- **Finding:** scoped destructive forms remain uncovered: `chmod -R 000 /`,
  `TRUNCATE` without `TABLE`, `docker volume rm`, and `npm publish`.
- **Issue:** [#188](https://github.com/IliasAlmerekov/aegis-shellguard/issues/188),
  sliced into `M5.1` [#189](https://github.com/IliasAlmerekov/aegis-shellguard/issues/189),
  `M5.2` [#190](https://github.com/IliasAlmerekov/aegis-shellguard/issues/190),
  `M5.3` [#193](https://github.com/IliasAlmerekov/aegis-shellguard/issues/193),
  `M5.4` [#191](https://github.com/IliasAlmerekov/aegis-shellguard/issues/191),
  `M5.5` [#192](https://github.com/IliasAlmerekov/aegis-shellguard/issues/192),
  `M5.6` [#194](https://github.com/IliasAlmerekov/aegis-shellguard/issues/194),
  `M5.7` [#195](https://github.com/IliasAlmerekov/aegis-shellguard/issues/195)
- **Traceability:** [plan](docs/plans/2026-07-14-m5-point-pattern-gaps.md);
  [ADR-014](docs/adr/adr-014-launcher-and-absolute-path-normalization-for-token-prefix-detection.md),
  [ADR-015](docs/adr/adr-015-destructive-sql-detected-by-regex-not-token-prefix.md).

### M6 — Project config can disable recovery

- **Finding:** project config could set a weaker snapshot policy or disable
  required confinement inherited from trusted config.
- **Traceability:** [ADR-013](docs/adr/adr-013-project-config-security-ratchet.md);
  commits `86f38ad`, `f4bd0a7`, `c834477`.

### M7 — Shell execution is not type-safe on audit readiness

- **Finding:** an audit setup failure can be represented as a successful helper
  result, leaving the execute-after-audit invariant dependent on control-flow
  convention.
- **Issue:** [#218](https://github.com/IliasAlmerekov/aegis-shellguard/issues/218)
- **Traceability:** [plan](docs/plans/2026-07-14-m7-audit-readiness-state.md);
  `src/shell_flow.rs`.

### M8 — Snapshot and Rollback wording implies post-effect recovery

- **Finding:** Git snapshots preserve pre-execution working-tree state; they do
  not capture a later command's deletion of clean tracked files, and no snapshot
  plugin is universal. Wording that promises to undo the dangerous command
  exceeds the product contract.
  Widened by [ADR-026](docs/adr/adr-026-snapshot-rollback-contract-for-1-0.md):
  §5 replaces the wording across `README.md`, `CONTEXT.md`, the TUI/explanation
  copy, the threat model, and the examples; §4 adds the disclosure behaviour when
  no provider applies.
- **Issue:** wording
  [#205](https://github.com/IliasAlmerekov/aegis-shellguard/issues/205);
  disclosure behaviour
  [#251](https://github.com/IliasAlmerekov/aegis-shellguard/issues/251)
- **Traceability:** [plan](docs/plans/2026-07-14-m8-snapshot-product-contract.md);
  [ADR-004](docs/adr/adr-004-snapshots-are-best-effort-audit-is-append-only.md);
  [ADR-026](docs/adr/adr-026-snapshot-rollback-contract-for-1-0.md).

### M9 — Snapshot identifiers do not round-trip through the rollback CLI

- **Finding:** composite tab-separated snapshot IDs render like columns and are
  not reliably copyable as the single `aegis rollback` argument.
  Fixed by [ADR-026](docs/adr/adr-026-snapshot-rollback-contract-for-1-0.md): the
  separator becomes `:` at format version `v3`, `v2` tab-joined ids stay parseable
  permanently, and `aegis snapshot list` prints a ready-to-use
  `aegis rollback '<id>'` line per row.
- **Issue:** [#215](https://github.com/IliasAlmerekov/aegis-shellguard/issues/215)
- **Traceability:** [plan](docs/plans/2026-07-14-m9-rollback-id-round-trip.md);
  `src/rollback.rs` and snapshot plugin ID parsers;
  [ADR-026](docs/adr/adr-026-snapshot-rollback-contract-for-1-0.md).

### M10 — README shows a snapshot before approval

- **Finding:** the Before/After example placed snapshot creation inside the
  confirmation dialog even though snapshots are created only after approval.
- **Traceability:** `README.md` Before/After and command-flow examples;
  `tests/snapshot_ordering.rs::test_denied_danger_command_records_no_snapshots`;
  [PR #120](https://github.com/IliasAlmerekov/aegis-shellguard/pull/120);
  [required CI run](https://github.com/IliasAlmerekov/aegis-shellguard/actions/runs/29342385519).

---

## P3 — Low / informational

The severity letter records the reviewer's original impact judgement and says
nothing about release weight: `#202` retired the old "P3 does not block" rule, so
some of the findings below are in the `1.0` milestone and some terminate in a
stated non-goal. Which is which is the state of the linked issue.

### P3-1 — SQLite snapshot creation has a TOCTOU window

- **Finding:** existence checks and copy are separate instead of reserving the
  target atomically.
- **Issue:** [#221](https://github.com/IliasAlmerekov/aegis-shellguard/issues/221)
- **Traceability:** [consolidated plan](docs/plans/2026-07-14-p3-follow-ups.md#p3-1--sqlite-snapshot-creation-toctou).

### P3-2 — Backslash-newline tokenization is underspecified

- **Finding:** shell line-continuation edge cases can diverge from scanner
  tokenization.
- **Disposition:** terminates in the ADR-010 non-goal — Aegis does not emulate a
  shell ([#202](https://github.com/IliasAlmerekov/aegis-shellguard/issues/202)).
- **Traceability:** [consolidated plan](docs/plans/2026-07-14-p3-follow-ups.md#p3-2--backslash-newline-tokenization).

### P3-3 — Parameterized IFS expansion remains opaque

- **Finding:** C2 covers literal `$IFS` / `${IFS}`, not `${IFS:-x}`,
  `${IFS:+x}`, or runtime reassignment.
- **Disposition:** terminates in the ADR-010 non-goal — Aegis does not emulate a
  shell ([#202](https://github.com/IliasAlmerekov/aegis-shellguard/issues/202)).
- **Traceability:** [consolidated plan](docs/plans/2026-07-14-p3-follow-ups.md#p3-3--parameterized-ifs-expansion).

### P3-4 — Renderer fallback is future fail-open

- **Finding:** the final wildcard renderer arm could auto-approve a future risk
  variant.
- **Issue:** [#219](https://github.com/IliasAlmerekov/aegis-shellguard/issues/219)
- **Traceability:** [consolidated plan](docs/plans/2026-07-14-p3-follow-ups.md#p3-4--renderer-fallback).

### P3-5 — Sandbox status is vulnerable to check/use drift

- **Finding:** recorded availability can diverge from confinement actually
  applied at execution.
- **Issue:** [#223](https://github.com/IliasAlmerekov/aegis-shellguard/issues/223)
- **Traceability:** [consolidated plan](docs/plans/2026-07-14-p3-follow-ups.md#p3-5--sandbox-status-toctou).

### P3-6 — Current-directory failure falls back to `.`

- **Finding:** snapshot planning can use `.` after `current_dir()` failure.
- **Issue:** [#220](https://github.com/IliasAlmerekov/aegis-shellguard/issues/220)
- **Traceability:** [consolidated plan](docs/plans/2026-07-14-p3-follow-ups.md#p3-6--current-directory-fallback).

### P3-7 — Optional Starlark dependencies carry unmaintained advisories

- **Finding:** `cargo audit` reports allowed unmaintained crates only through the
  opt-in `starlark-policy` feature.
- **Disposition:** eliminated rather than accepted — the DSL is deleted from the
  tree, so the advisory chain leaves with it
  ([#222](https://github.com/IliasAlmerekov/aegis-shellguard/issues/222),
  [ADR-028](docs/adr/adr-028-the-starlark-policy-dsl-is-removed-before-1-0.md)).
- **Issue:** [#225](https://github.com/IliasAlmerekov/aegis-shellguard/issues/225)
- **Traceability:** [consolidated plan](docs/plans/2026-07-14-p3-follow-ups.md#p3-7--optional-starlark-advisories).

### P3-8 — Destructive SQL has known coverage limits

- **Finding:** SQL comments as separators and additional destructive verbs/CLI
  programs remain outside current ADR-015 patterns.
- **Disposition:** terminates in the ADR-010 non-goal — Aegis does not emulate a
  shell, and SQL parsing stays out of scope
  ([#202](https://github.com/IliasAlmerekov/aegis-shellguard/issues/202)).
- **Traceability:** [consolidated plan](docs/plans/2026-07-14-p3-follow-ups.md#p3-8--destructive-sql-follow-ups).

### P3-9 — Inline script bodies are regex-scanned twice

- **Finding:** `Scanner::assess` scans each recursive target, then rebuilds an
  effective target from the same tokens (`effective_token_slices` →
  `join(" ")` → `full_scan`) whose only difference is program-basename
  normalization, so a large inline body pays the full regex set twice. Cost is
  linear at ~118 µs/KB, bounded by `MAX_INLINE_SCRIPT_LEN`, which puts a
  just-under-cap body at ~1.9 ms — inside the `< 2 ms` budget with little margin.
- **Disposition:** in-budget performance, not a promise falsified — the worst case
  stays inside the `< 2 ms` hot-path gate, so it does not block 1.0
  ([#202](https://github.com/IliasAlmerekov/aegis-shellguard/issues/202)).
- **Traceability:** [rebaseline evidence](docs/performance-baseline.md#heredoc_worst_case-rebaseline-2026-08-04);
  `crates/aegis-scanner/src/scanner/assessment.rs` effective-slice loop;
  [ADR-014](docs/adr/adr-014-launcher-and-absolute-path-normalization-for-token-prefix-detection.md);
  `perf/scanner_bench_baseline.toml`.

---

## Confirmed strengths retained from the audit

- Intrinsic `Block` remains unbreakable by allowlist, rules, mode, or CI policy.
- Classification, config, policy, confirmation, and hook failures are intended to
  fail closed.
- Aegis remains a heuristic command guardrail, not a sandbox or backup system.
