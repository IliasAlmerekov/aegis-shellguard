# ADR-033 — The Diagnostic stream goes to stderr and is not a contract

## Status

Accepted. Partially supersedes
[ADR-023](adr-023-hook-panic-fails-closed-in-two-layers.md): its "no
subscriber" claim no longer holds in general, but the two-layer panic
containment it describes remains in force unchanged, because `Hook` mode
keeps this stream off by default.

## Context

The binary never installed a `tracing` subscriber, so all 49 `tracing::`
events across the library crates were discarded. The motivating case:
`SnapshotRegistry::snapshot_all` (`crates/aegis-snapshot/src/registry.rs`)
logs a warning when one plugin's snapshot attempt fails and the registry moves
on to the next plugin. That warning reached nobody. An operator watching a
Danger command auto-approve had no way to learn that Snapshot coverage was
partial.

Scope is observability only. This ADR does not change the confirmation
dialog, snapshot ordering, or the Required-recovery barrier (ADR-031); those
are separate follow-up issues.

## Decision

`src/diagnostics.rs` installs a `tracing-subscriber` `fmt` layer, called from
`src/main.rs` after the two pre-clap short-circuits (the language-worker check
and the Linux Landlock wrapper check) and before the Tokio runtime is built,
so those two minimal processes never pay for it.

The writer is always stderr. Seven surfaces own stdout as a machine protocol
(the language-worker framed protocol, `hook` JSON, `watch` NDJSON, `--output
json`, `audit --format json|ndjson`, `config show`, and `config validate
--output json`), and `tracing_subscriber::fmt`'s default writer is stdout, so
the writer is overridden explicitly.

Two surfaces are silent regardless of level, for different reasons. The
internal language-worker mode exits before `src/diagnostics.rs` is ever
called (`src/main.rs`'s worker short-circuit runs before the install point
described below), so it has no Diagnostic stream under any setting,
including `AEGIS_LOG`: it is a parse-only process with a framed protocol on
stdout, and stray output would corrupt the frame stream (ADR-022 §2). The
`hook` subcommand does install the subscriber, but keeps it off by default,
because no human reads its stderr and byte-pinned tests assert its exact
contents (`CONTAINED_PANIC_STDERR` in `src/install/hook.rs`, and
`tests/agent_hooks_m4.rs`); setting `AEGIS_LOG` turns the stream on there.

The default level is `warn`. The existing `--verbosity <quiet|standard|
verbose>` / `--quiet` / `-v` flags set the base level (`error` / `warn` /
`info`). The `AEGIS_LOG` environment variable, parsed as an `EnvFilter`,
overrides the flag-derived level entirely when set. `RUST_LOG` is never read:
Aegis runs inside other projects' environments, where that variable is
usually set for something else. The flags are written by the agent; the
environment variable is the human's only handle on an already-running proxy.

The line format is compact: no timestamp, no target, no ANSI colour, prefixed
`aegis:` to match the existing `CONTAINED_PANIC_STDERR` style. A timestamp
would make output nondeterministic against the repo's byte-exact stderr
assertions.

A formatting layer truncates every field value at 512 bytes, cutting on a
UTF-8 character boundary and marking the cut, because
`crates/aegis-snapshot/src/docker/mod.rs` puts a subprocess's verbatim stderr
into a field with no bound otherwise. The truncation lives in the layer, once,
rather than at each call site, so the next `tracing::warn!` inherits it for
free.

The `cwd` field is removed from the git-rollback-conflict error in
`crates/aegis-snapshot/src/git.rs`: the agent already knows its own cwd, and
in `Hook` mode it can differ from what the agent thinks it is. The absolute
paths already logged in `sqlite.rs`, `mysql/mod.rs`, `postgres/mod.rs`, and
`supabase/runtime/mod.rs` stay: they point at Aegis's own snapshot artifacts,
and the full path is what makes the warning actionable.
`SnapshotRegistry::snapshot_all` still does not log the raw `cmd` string it
receives.

CONVENTION.md §2 gains two rules: a `tracing` field must never carry a raw
command string or an environment variable value, and an event's level follows
one test — it is `warn` if, after it fires, an operator who believes Snapshot
coverage is complete would be wrong. A plugin correctly deciding it has
nothing to do (for example, `docker ps` failing because no daemon is running)
is not a coverage degradation and stays `info`; that call site was previously
`warn` and is demoted here. The first rule is guarded by
`tracing_fields_never_carry_raw_command_or_env_value` in
`tests/architecture_source_rules.rs`, modeled on the existing
`decision_engine_is_pure_no_io` source-grep test.

`aegis_snapshot::SNAPSHOT_FAILED_CONTINUING` names the "snapshot failed,
continuing" message as a `pub const`, exported from the crate root and
referenced by both the emission site and its tests — modeled on
`CONTAINED_PANIC_STDERR` in `src/install/hook.rs`. The Diagnostic stream as a
whole carries no contract, but this specific string is deliberately pinned so
that dependence on it is explicit rather than accidental.

`docs/troubleshooting.md` documents `AEGIS_LOG` next to `AEGIS_REAL_SHELL`.

## Consequences

- The Wrapper is no longer byte-transparent on stderr whenever the Diagnostic
  stream is non-empty. In `tests/full_pipeline_shell.rs`,
  `shell_wrapper_ls_nonexistent_matches_real_shell_passthrough` and
  `shell_wrapper_preserves_environment_and_working_directory` diff Aegis's
  stderr against `/bin/sh` byte-for-byte on the safe-command path; a new line there
  on that path is a bug in this feature, not a reason to relax the test.
- This makes plugin and recovery failures visible to the operator, but it does
  not change what the human sees at the moment of decision: Snapshot creation
  happens after the confirmation dialog (`src/shell_flow.rs`, the
  confirm-then-snapshot ordering documented there), so a Danger command is
  still approved before Aegis knows whether its Snapshot will succeed.
  Surfacing partial Snapshot coverage at confirmation time is a separate
  follow-up issue.
- The no-raw-command-or-env-value grep guard is a floor, not a proof: it
  cannot see a path that arrives already formatted through a `Display` impl on
  an error type such as `SnapshotError`. A value assembled that way and passed
  to `tracing` through `%err` would not match the guard's literal patterns.
