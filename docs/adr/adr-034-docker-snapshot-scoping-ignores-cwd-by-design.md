# ADR-034 — Docker snapshot scoping ignores `cwd` by design; the flake it looked like is a test-isolation gap

## Status

Accepted.

## Context

A bug report (issue in the diagnostic-stream branch's review queue) argued that
`watch_recovery_prompt_run_once_executes_and_audits_degradation` and
`watch_recovery_prompt_deny_prevents_execution_and_audits_degradation`
(`src/watch/runner/tests.rs`) are flaky under a full workspace `cargo test`
because `DockerPlugin::is_applicable` (`crates/aegis-snapshot/src/docker/mod.rs`)
ignores the `cwd` it is given and asks `docker ps` whether *any* container is
running anywhere on the host. The theory: `aegis-snapshot`'s own test binary
spins up real containers via `docker run -d`, and if that window overlaps with
one of these two tests calling `create_watch_snapshots` against a bare
`TempDir`, `DockerPlugin` reports itself applicable, `recovery_status` returns
`Ready` instead of `Degraded(NoSnapshotAvailable)`, and the recovery prompt
never fires.

Two of the three premises don't hold against the current tree:

- `crates/aegis-snapshot`'s own `docker` unit tests never touch a real Docker
  daemon — every one of them runs against a `write_mock_docker` shell script
  substituted for the `docker` binary (`crates/aegis-snapshot/src/docker/tests.rs`).
- The one test suite that *does* start real containers,
  `tests/docker_integration.rs`, is gated behind `AEGIS_DOCKER_TESTS=1` and is
  skipped by default — including under the plain `cargo test` that
  `.githooks/pre-push` runs.
- `DockerPlugin`'s default scope is not "any container": `DockerScope::default()`
  (`crates/aegis-config/src/snapshot.rs`) is `Labeled` with label
  `aegis.snapshot`, so `is_applicable` under the default `AegisConfig` filters
  `docker ps` to containers explicitly opted in via
  `--label aegis.snapshot=true`. An unrelated container on the host — including
  one left over from a developer's own `docker compose up` — does not make the
  plugin applicable.

So a plain `cargo test` run cannot reproduce the reported race today: nothing in
the default test run creates a labeled container, and nothing outside the
opt-in integration suite touches the real daemon. The two watch-recovery tests
were still non-deterministic in one narrow sense worth closing anyway: they
built a `RuntimeContext` from `AegisConfig::default()`, which means the real
`DockerPlugin` sat in the registry pointed at whatever `docker ps` says on the
machine actually running the test. On a developer box that already runs an
`aegis.snapshot=true`-labeled container for unrelated reasons — a persistent
compose stack, say — the test would silently start depending on that host
state instead of asserting "no snapshot plugin applies to this cwd".

## Decision

1. **Keep `DockerPlugin::is_applicable` ignoring `cwd`.** Scoping by directory
   doesn't map onto how containers relate to a project — a container has no
   filesystem-path relationship to the shell's cwd, and `docker compose`
   projects, remote build contexts, and manually-started containers all break
   a path-based check. `DockerScope` (label or name-pattern filtering) is the
   intended scoping mechanism, and its default (`Labeled`, opt-in) is already
   the conservative choice: a container is eligible for Aegis snapshots only
   after someone puts `aegis.snapshot=true` on it. No cwd check needed —
   ADR-004's "best-effort" framing already accepts breadth here. Not now, and
   not without a redesign of what "applicable" would even mean for `Names` or
   `All` scope.
2. **Close the residual determinism gap anyway.** Added
   `RuntimeContext::set_snapshot_registry_for_tests` (test-only,
   `src/runtime/context.rs`), which pins the lazily-built snapshot registry to
   an explicit plugin list before anything can materialize the real one from
   config. The two watch-recovery tests now build their `RuntimeContext` with
   a registry containing only `GitPlugin` — deterministically inapplicable to
   a fresh `TempDir` outside any repository — so the asserted degradation
   traces to "no snapshot plugin applies here", not to whatever the host's
   Docker daemon happens to be running.
3. **`sqlite`/`mysql`/`postgres`/`supabase` need no change today.** Their
   `is_applicable` also ignores `_cwd`, but each is gated on a config value
   that defaults empty:
   - `sqlite`: `crates/aegis-snapshot/src/sqlite.rs` — `db_path.as_os_str().is_empty()`
   - `mysql`: `crates/aegis-snapshot/src/mysql/mod.rs` — `database.is_empty()`
   - `postgres`: `crates/aegis-snapshot/src/postgres/mod.rs` — `database.is_empty()`
   - `supabase`: `crates/aegis-snapshot/src/supabase/runtime/mod.rs` — `config.db.database.trim().is_empty()`

   Under `AegisConfig::default()` none of them can ever be applicable, so they
   carry no host-state race today. If a future change ever gives one of those
   fields a non-empty default, revisit this ADR — the same registry-pinning
   test pattern from item 2 applies.

## Consequences

- The two watch-recovery tests no longer read real host Docker/database state,
  even though nothing in the current default-config test run was actually
  doing so — this removes a latent footgun for the next engineer who changes
  `AegisConfig::default()`.
- `DockerPlugin::is_applicable` keeps its current signature and behavior;
  `_cwd` stays unused by design, not oversight.
- No product change ships from this ADR. If cwd-scoped Docker snapshotting
  becomes a real ask (e.g. "only snapshot containers started by *this*
  `docker-compose.yml`"), that's a new `DockerScopeMode` variant, filed
  separately — not a fix to `is_applicable`'s current contract.

## Addendum (2026-09-16): item 2 covered two tests, missed a third helper

Item 2 pinned the registry for `prepared_with_audit_path`, the helper shared by
`watch_recovery_prompt_deny_prevents_execution_and_audits_degradation` and
`watch_recovery_prompt_run_once_executes_and_audits_degradation`. A third test,
`watch_recovery_deny_records_enabled_sandbox_as_not_attempted`, builds its
`RuntimeContext` through a separate helper, `prepared_with_optional_sandbox`,
which was never updated to call `set_snapshot_registry_for_tests`. On a
machine where a real Docker daemon reports itself reachable, that helper's
default registry lets `DockerPlugin` — not `GitPlugin` — decide applicability,
so `recovery_status` can resolve to `Ready` instead of the
`Degraded(NoSnapshotAvailable)` the test expects, and the sandbox-status and
prompt assertions built on top of it fail nondeterministically depending on
host state rather than test fixtures.

`prepared_with_optional_sandbox` now pins the same `GitPlugin`-only registry as
`prepared_with_audit_path`. Both `RuntimeContext` builders in
`src/watch/runner/tests.rs` follow the pattern from item 2; no other helper in
that file constructs a `RuntimeContext` without it.

## Addendum (2026-09-16): `GitPlugin`-only pinning wasn't the end of it — a fail-open check reopened the gap

A separate report against
`watch_recovery_deny_records_enabled_sandbox_as_not_attempted` (the same test
the first addendum pinned to `GitPlugin`) surfaced after
`GitPlugin::is_applicable` changed to fail open on spawn failure: when
`Command::status()` for `git rev-parse --git-dir` returns `Err` (no signal
either way, e.g. process/FD pressure from many concurrent test binaries under
a full workspace `cargo test`), the plugin now reports itself applicable
instead of not. The report reproduced twice via `.githooks/pre-push`'s real
`git push` but not in isolated single-test runs, and hypothesized that this
newly fail-open check was what flipped the test's outcome under load.

Two things were checked before touching the test:

- Five consecutive full-workspace `cargo test` runs with
  `RUST_LOG=aegis_snapshot=error` never failed and never logged the
  "failed to spawn git while checking applicability" line — the spawn-failure
  branch did not fire in this environment.
- Tracing the plugin's actual code path: fail-open in `is_applicable` only
  changes whether `GitPlugin::snapshot` gets *attempted*. On a bare `TempDir`
  with no `.git`, that attempt's own `git status --porcelain` call still
  fails (real non-repo directory), so `SnapshotRegistry::snapshot_all` logs
  `SNAPSHOT_FAILED_CONTINUING` and records nothing — the same empty result as
  before the fail-open change. A spawn failure at the applicability check,
  by itself, cannot turn an empty snapshot list into a populated one, and the
  test's `sandbox_status` assertion only diverges from `not_attempted` when a
  snapshot *is* recorded. So the fail-open branch was not confirmed as a
  standalone cause of the reported failure; whatever produced it (if the
  report reproduces again) is a distinct, so-far-unidentified path.

Regardless of that open question, `prepared_with_optional_sandbox` had no
reason to depend on `GitPlugin` in the first place: the test asserts what
`sandbox_status` gets audited on a Recovery-Deny path, not anything about the
Git plugin. Pinning it to `SnapshotRegistry::new_with_plugins(vec![])` removes
every `git` spawn from the test — including the one that fails open — so it
can no longer inherit *any* race in `GitPlugin::is_applicable`, confirmed or
not. `prepared_with_audit_path` keeps its `GitPlugin`-only registry, since the
two tests built from it assert the `no_snapshot_available` degradation
specifically because no plugin applies — an empty registry would make that
assertion trivially true instead of testing anything.
