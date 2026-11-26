# Architecture

Aegis is a Rust CLI that runs as the `$SHELL` of an AI coding agent. It parses each command before the real shell sees it, classifies the risk, and either lets it through, asks a human, or blocks it. Dangerous commands get a best-effort snapshot first, run inside an OS sandbox, and every decision lands in an append-only audit log.

`CONTEXT.md` defines the domain terms. `PRD.md` is the product promise. `CONVENTION.md` holds the enforced rules. This file maps the code. A change to the structure below edits this file in the same pull request.

## Workspace layout

`Cargo.toml` lists the library crates in `[workspace].members`. They live under `crates/`. The `aegis` package at the repository root is the binary and a thin library. Dependencies point toward `aegis-types`, and no crate depends on the root package. `tests/architecture_boundaries.rs` checks these edges against each crate's `Cargo.toml`.

### Crates

- `aegis-types` holds the shared vocabulary: risk levels, pattern shapes, `Assessment`, `Decision`, snapshot records, and the exit codes. It depends on no other Aegis crate.
- `aegis-parser` tokenizes shell commands and resolves wrappers, `env` launchers, and heredocs. It depends on `aegis-types`.
- `aegis-scanner` matches the built-in and custom patterns against parsed commands and produces an `Assessment`. It depends on `aegis-parser` and `aegis-types`.
- `aegis-policy` evaluates the policy rules over an assessment and returns a decision. It is pure and does no I/O. It depends on `aegis-parser` and `aegis-types`.
- `aegis-config` owns the config model, the layered loader, validation, the sandbox write ceiling, allowlist and blocklist matching, and the JSON schema. It depends on `aegis-parser`, `aegis-scanner`, and `aegis-types`.
- `aegis-explanation` builds the structured explanation of a decision. It depends on `aegis-config`, `aegis-policy`, and `aegis-types`.
- `aegis-tui` renders the confirmation dialog and the non-interactive fallback. It depends on `aegis-config`, `aegis-explanation`, `aegis-parser`, and `aegis-types`.
- `aegis-audit` writes the JSONL audit log with an optional SHA-256 hash chain, rotation, and integrity verification. It depends on `aegis-config`, `aegis-explanation`, and `aegis-types`.
- `aegis-snapshot` defines the snapshot plugin trait and the registry, with the git, docker, postgres, mysql, sqlite, and supabase backends. It also handles rollback. It depends on `aegis-config` and `aegis-types`.
- `aegis-sandbox` prepares the confined command: bwrap plus Landlock on Linux, Seatbelt on macOS. Native Windows is unsupported. It depends on `aegis-types`.
- `aegis-language` is the leaf crate for language-aware analysis. It owns the Tree-sitter parsing, the grammar manifest, the worker wire protocol, and the language adapters. The grammars compile only with the `language-analysis` feature. `tests/aegis_language_boundary.rs` guards the boundary.

### Binary and library modules

The library (`src/lib.rs`) exports these modules:

- `analysis` runs language-aware analysis on the parent side: it finds analyzable source in a command (`router`), spawns the ephemeral worker (`worker_client`), enforces deadlines, and turns worker failures into typed degradations.
- `planning` wraps the pure policy engine. It builds the `InterceptionPlan` for a command: the decision, the snapshot plan, and the git targets resolved from the working directory.
- `runtime` builds the `RuntimeContext` once: loaded config, audit logger, snapshot registry, sandbox settings, and recovery state. Its `disposition` module holds the flow after the policy disposition is known, shared by the shell path and watch: prompt and rule persistence, the approve-always and Recovery gate, block display, and the `SandboxError` mapping. Every audit write, shell or watch, goes through `RuntimeContext::append_audit_entry` with one `AuditWriteOptions`. Each path implements `DispositionOutput` for its own prompts and keeps its own execution step.
- `explanation` formats a `CommandExplanation` for humans.
- `watch` implements the NDJSON watch protocol and its runner. `exec` streams the child's output as frames, and `sandbox` prepares the confined child for a frame.
- `update` is the opt-in npm update notice. Its consent and cache state live under `~/.aegis/`, never in a project file.
- `toggle` turns interception on and off. `runtime_gate` detects CI. `error` holds the top-level error type.

The binary (`src/main.rs`) adds the entry points:

- `cli_dispatch` and `cli_commands` parse arguments and route subcommands.
- `shell_wrapper` and `shell_compat` handle the `$SHELL` mode: parse `-c`, plan, then run.
- `execution` carries out a plan: confirmation, snapshots, sandbox preparation, audit write, then exec.
- `install` registers the hook for Claude and Codex and installs the shell wrapper.
- `rollback`, `rollback_last`, `verify`, `prune`, `diagnostics`, and `policy_output` back the matching subcommands.

Two more binaries ship in `src/bin/`: `aegis_schema` writes the config schema, and `aegis_benchcheck` checks benchmark budgets.

## Command flow

A command arrives through the shell wrapper, the agent hook, or watch mode. The parser and scanner produce an assessment. Optional language analysis can raise it. `planning` applies the policy rules and the allowlist, then picks a decision. For a command that needs approval, `aegis-tui` asks the human. Approved dangerous commands get a snapshot, then `aegis-sandbox` confines them, and the command runs. The audit entry is written on every path, including blocks.

## Security invariants

### Fail-closed

A failure never turns into a silent allow. The result is an explicit block, or an explicit human approval before execution. The code does this at each stage:

- `aegis-parser` flags truncated or over-nested input, and callers treat the flag as unresolved rather than safe. The scanner does the same when its work caps are exceeded (`crates/aegis-scanner/src/scanner/assessment.rs`).
- `aegis-config` returns an error for an unreadable or invalid config, and `RuntimeContext::load` in `src/runtime/context.rs` propagates it. The process then exits with the internal-error code instead of running with defaults. A leftover `policy.star` file also fails the load (`crates/aegis-config/src/validate.rs`).
- `aegis-tui` denies when no human is at the keyboard, and it denies any risk level it does not recognize (`crates/aegis-tui/src/stdout_renderer.rs`).
- `src/execution/mod.rs` blocks the command when the sandbox is required but unavailable, or when sandbox setup fails. If the audit write fails on those paths, the command does not run either.
- `aegis-sandbox` fails closed in the Landlock wrapper on a kernel without support or on a malformed request, so the program is never exec'd unconfined.
- `src/install/hook.rs` catches a panic in the hook and denies, so a crash cannot approve a command.
- `src/analysis` turns a worker failure or timeout into a degradation that requires approval. It never turns it into a pass.

`Block`-level commands cannot be overridden by the allowlist, CI mode, or a project config.

### Project config ratchet

Config loads in layers: the global `~/.aegis/config.toml` is trusted, and a project `.aegis.toml` is not. A project file can only tighten security fields. The code is `crates/aegis-config/src/model/ratchet.rs` and its submodules, run by `merge_layer` in `crates/aegis-config/src/model/merge.rs`.

Every config field declares one direction in `model/ratchet/direction.rs`. `merge_layer` destructures `AegisConfig` and each nested struct exhaustively, so adding a field without choosing a direction does not compile. A project value that would weaken a field is ignored and reported as a `project_security_ratchet` warning. The directions in use:

- Tighten keeps the stricter of the trusted and requested value. This covers `mode`, `allowlist_override_level`, `snapshot_policy`, `ci_policy`, the six `auto_snapshot_*` flags, `sandbox.allow_network`, `audit.rotation_enabled`, `audit.max_file_size_bytes`, `audit.retention_files`, `audit.integrity_mode`, `prune.enabled`, and the numeric `language_analysis` budgets. The budgets are also capped at every layer, global included.
- Narrow-only custom rules cover `sandbox.allow_write` (path-tree intersection), `[[rules]]` (project `Allow` entries are dropped), the prune limits, `docker_scope`, and the provider targets `postgres_snapshot`, `mysql_snapshot`, `sqlite_snapshot_path`, and `supabase_snapshot`. Once the trusted layer enables a provider target, the project cannot change any field of it.
- Append covers `allow`, `block`, and `custom_patterns`. Concatenation cannot remove a trusted entry, and each entry keeps its layer for later checks.
- Global-only covers `language_analysis.trusted_aliases`.
- Unratcheted covers `config_version` and `audit.compress_rotated`, which carry no security weight.
- `sandbox.enabled` and `sandbox.required` are removed. A config that still sets either fails to load as an unknown field.

## Config schema

`aegis-schema.json` at the repository root is the schema for the config file. It is generated from the `AegisConfig` type with `schemars`: `cargo run --bin aegis_schema` runs `src/bin/aegis_schema.rs`, which writes the file. CI regenerates it and fails when the committed copy differs. Edit the Rust types, never the JSON.
