# Aegis — Transformation Roadmap

This document records **the path taken** — the phases the project moved through,
their deliverables, and their definitions of done. It is not the 1.0 scope and it
holds no release gate.

- The normative 1.0 product promise is [`PRD.md`](PRD.md).
- The live release gate is the
  [`1.0` milestone](https://github.com/IliasAlmerekov/aegis-shellguard/milestone/1)
  and nothing else
  ([ADR-027](docs/adr/adr-027-one-1-0-release-gate-lives-in-the-issue-tracker.md)).
- Where a phase below states a goal that a later decision changed, the PRD wins.
  The phase text is kept as written, with the superseding decision named.

The north star this roadmap was written against: Aegis intercepts every shell
command, classifies it with zero false negatives, persists every human decision as
a machine-readable rule, and confines execution at the OS level — all in under
2 ms on the hot path. The confinement half was optional when the roadmap was
written and is a mandatory 1.0 layer under
[ADR-029](docs/adr/adr-029-the-sandbox-is-a-mandatory-1-0-layer.md); see Phase 6.

---

## Phase 0 — Foundation Repair

**Goal:** eliminate the critical defects that undermine Aegis's core value
proposition. Nothing in Phase 1+ is safe to build until these are resolved.
These are not cleanups — they are open security and correctness bugs.

### 0.1 Async correctness in snapshot plugins

- Replace `std::thread::sleep` with `tokio::time::sleep` everywhere in retry loops
  (`docker.rs`, `postgres.rs`, `mysql.rs`).
- Change `SnapshotPlugin::is_applicable` trait signature from `fn` to
  `async fn` — or remove blocking I/O from all implementations entirely.
  Either decision must be consistent across the trait and all six plugins.
- Remove `spawn_blocking` workarounds that exist only because `is_applicable`
  wasn't async.

**Done when:** `cargo clippy` reports no `blocking_in_async_context` equivalents;
`tokio::test` with `#[timeout]` passes for all snapshot plugins.

### 0.2 Audit log is a security artifact — treat it as one

- Emit a hard error (not a `tracing::warn`) on every audit write failure,
  regardless of the `verbose` flag.
- Change the default for `AuditIntegrityMode` from `Off` to `ChainSha256`.
  SHA-256 hash chaining is Aegis's integrity check for corruption and
  inconsistent edits; it must be
  opt-out, not opt-in.
- Add `#[must_use]` to `AuditLogger::append` and every `Result`-returning
  public function in the audit module.

**Done when:** deliberately breaking the audit file path causes a non-zero exit
with a user-visible error; a new install's default config has `integrity_mode =
"ChainSha256"`.

### 0.3 Eliminate dead code in security-critical paths

- Remove `#[allow(dead_code)]` from `src/error.rs` and `src/interceptor/parser/mod.rs`.
  Either use the flagged code or delete it. There is no middle ground for dead
  code in a hot-path security parser.
- Remove `#[allow(clippy::too_many_arguments)]` from `append_watch_audit_entry`
  by introducing a `WatchAuditContext` value type that aggregates its 11 parameters.

**Done when:** `cargo clippy -- -D dead_code` passes with no suppressions in
`src/interceptor/` and `src/audit/`.

### 0.4 Harden config loading

- Fix `detect_effective_user_from_id_command`: resolve `id` via `PATH` lookup,
  not the hardcoded `/usr/bin/id` path.
- Fix `default_snapshots_dir`: replace the `HOME` fallback of `"."` with an
  explicit error when `HOME` is unset.
- Fix `custom_pattern_cache_key`: introduce a typed `CacheKey` newtype; validate
  that pattern fields do not contain the separator characters at construction time.
- Add a config migration path: `deserialize_config_version` must emit a
  structured migration error (not a parse failure) when `config_version > 1`,
  and must document the upgrade procedure.

**Done when:** all four issues have regression tests; CI passes on a config file
with `HOME` unset.

### 0.5 Declare MSRV and add Windows to CI

> **Superseded by the 2026 decision to drop native Windows:** native Windows is out of
> scope — Windows is supported only inside WSL2 (see `docs/platform-support.md`,
> `README.md`). The Windows-CI portion of this item no longer applies; the MSRV
> portion stands.

- Add `rust-version` to `Cargo.toml` (`1.80` was the minimum for
  `std::sync::LazyLock`; the declared MSRV has since moved on — `Cargo.toml` is
  the authority).
- ~~Add a `windows-latest` job to `.github/workflows/ci.yml`~~ (dropped — no
  native Windows build).

**Done when:** MSRV declared and enforced. (Windows-job gate withdrawn.)

---

## Phase 1 — Scanner Modernization

**Goal:** replace the current regex-on-raw-string scanner with a token-prefix
engine that is semantically correct, faster on the hot path, and extensible.
Inspired by codex's `execpolicy` crate.

### 1.1 Command tokenizer as the single source of truth (Done)

The scanner currently applies patterns to the raw command string. This produces
false positives (`echo "rm -rf"` triggers `rm -rf` patterns) and false negatives
(quoting tricks bypass substring matches).

Introduce a dedicated tokenizer that always runs first:

```
raw command string
    → tokenize (shlex)
    → ParsedCommand { program: &str, argv: &[&str] }
    → pattern matching on tokens
```

`ParsedCommand` becomes the canonical representation throughout the codebase.
The raw string is only used for display and audit logging.

### 1.2 Replace `HashMap<id, pattern>` with `MultiMap<program, Rule>` (Done)

Index rules by the first token of the command (the program name). This gives O(1)
lookup per command instead of scanning every pattern.

```rust
// Before: scan all N patterns for every command
patterns.iter().filter(|p| p.regex.is_match(raw_cmd))

// After: fetch only patterns relevant to this program
rules_by_program.get_vec("git")  // returns only git-* rules
```

For commands where the program cannot be determined (e.g. variable expansion),
fall back to a small set of universal patterns.

### 1.3 `PrefixRule` — token-level pattern matching(Done)

Replace free-form regex with token-prefix rules:

```rust
pub struct PrefixRule {
    pub pattern: PrefixPattern,   // ["git", "push", Alts(["--force", "-f"])]
    pub risk:    RiskLevel,
    pub justification: Option<Cow<'static, str>>,
}

pub enum PatternToken {
    Single(Cow<'static, str>),
    Alts(Vec<Cow<'static, str>>),
}
```

`Alts` lets one rule cover semantic equivalents (`--force` / `-f`) without
duplicating entries.

### 1.4 `justification` surfaces in the TUI (Done)

Every rule gains an optional human-readable explanation of _why_ it is risky:

```
⚠  git push --force

This command rewrites remote history. Collaborators with local copies
will have diverged refs and will need to force-pull or re-clone.
Consider --force-with-lease to at least detect concurrent pushes.

[A]llow  [D]eny  [Always allow]  [Always deny]
```

The `justification` field is shown in the confirmation dialog. It is set for
all built-in rules and can be added to user-defined rules in config.

### 1.5 `match_examples` / `not_match_examples` as first-class rule fields (Done)

Rules self-document and self-test:

```rust
pub struct PrefixRule {
    // ...
    pub match_examples:     &'static [&'static str],
    pub not_match_examples: &'static [&'static str],
}
```

At startup (in debug builds and tests), the scanner validates all built-in rules
against their examples. A rule that fails its own examples is a compile-time
error.

**Done when:** `cargo test` exercises all 70+ built-in rules against their
examples; the TUI shows `justification` text for all built-in `Warn` and `Danger`
rules; `cargo criterion` shows hot-path latency unchanged or improved.

---

## Phase 2 — Decision Persistence

**Goal:** when a human makes a decision about a command, that decision is
automatically persisted as a rule. The user never sees the same prompt twice for
the same command pattern. Inspired by codex's `amend.rs`.

### 2.1 "Always allow" writes a rule to config(Done)

When the user chooses "Always allow" in the TUI, Aegis:

1. Tokenizes the command into its prefix (program + meaningful flags, stripping
   variable arguments like file paths).
2. Calls `amend::append_allow_rule(config_path, &prefix)` which appends a new
   rule to `~/.aegis/aegis.toml` (or the active project config).
3. Invalidates the scanner cache so the new rule takes effect immediately.

The appended rule is human-readable TOML:

```toml
[[allow]]
pattern = ["git", "push", "--force-with-lease"]
reason  = "Approved by user on 2025-05-22"
```

### 2.2 "Always deny" writes a block rule(Done)

Same mechanism for the "Always deny" choice:

```toml
[[block]]
pattern = ["rm", "-rf", "/"]
reason  = "Blocked by user on 2025-05-22"
```

### 2.3 Rule deduplication on write(Done)

`amend` checks whether an equivalent rule already exists before appending.
A duplicate rule is silently skipped; a conflicting rule (same pattern, different
decision) produces a warning with the existing rule's location.

### 2.4 Allowlist merges into the unified rule system(Done)

The current `allowlist` config field (`allowed_commands`, `allowed_patterns`) is
deprecated and replaced by the `[[allow]]` rule table. A migration function reads
the old format on first load and writes the equivalent `[[allow]]` entries. The
old field is accepted but emits a deprecation warning.

**Done when:** after one interactive session, `~/.aegis/aegis.toml` contains the
user's allow/block decisions as typed rules; those decisions are respected on the
next run without re-prompting; the old allowlist format still loads with a warning.

---

## Phase 3 — Module Architecture

**Goal:** enforce strict module boundaries through the type system and directory
structure. Eliminate the monolithic files that have grown beyond 800 lines. Update
all documentation to match the actual module layout.

### 3.1 File size budget — hard limit 800 LoC(Done)

The following files exceed the 800-line limit and must be split:

| File             | Current size | Target                                                  |
| ---------------- | ------------ | ------------------------------------------------------- |
| `runtime.rs`     | ~1100 lines  | `runtime/context.rs` + `runtime/user.rs`                |
| `decision.rs`    | ~900 lines   | `decision/engine.rs` + `decision/types.rs`              |
| `explanation.rs` | ~800 lines   | `explanation/formatter.rs` + `explanation/templates.rs` |
| `install.rs`     | ~1600 lines  | `install/` submodule (3–4 files)                        |
| `watch.rs`       | ~1100 lines  | `watch/` submodule (loop + protocol)                    |

Rule: when extracting a file, move its tests and type docs into the new file.
Never leave tests behind in the old file for code that moved.

### 3.2 Update `ARCHITECTURE.md` to match reality(Done)

`ARCHITECTURE.md` describes seven layers. The actual code has grown to include
`planning/`, `toggle.rs`, `runtime_gate.rs`, `shell_flow.rs`, and five additional
snapshot backends. Update the document to be authoritative again:

- Add `planning/` to the policy engine layer.
- Add `toggle.rs` and `runtime_gate.rs` to the entrypoint layer.
- Add all six snapshot backends to the snapshot layer description.
- Document the `watch` mode NDJSON protocol as a first-class protocol (currently
  only mentioned in passing).

### 3.3 `AuditEntry` — typed variant instead of flat struct(Done)

Replace the 18-field flat struct with a typed enum:

```rust
pub enum AuditEntry {
    Decision(DecisionEntry),   // always-present fields + decision outcome
    Watch(WatchEntry),         // watch-mode source, cwd, exit code
}

pub struct DecisionEntry {
    pub timestamp: DateTime<Utc>,
    pub command:   String,
    pub risk:      RiskLevel,
    pub decision:  Decision,
    // ... 4-5 always-present fields, no Option<T>
}
```

This makes it impossible to construct a decision entry without a `risk`
level.  Watch fields (`source`, `cwd`, `id`) remain `Option<String>` so that
legacy audit log lines which omit them still deserialize correctly.

### 3.4 `AegisConfig` — remove type alias ambiguity(Done)

Remove `pub type Config = AegisConfig`. All code uses `AegisConfig` directly.
Generate a JSON schema from the type (`just write-config-schema`) so editors can
validate `aegis.toml` files with autocompletion.

**Done when:** no file in `src/` exceeds 800 lines; `ARCHITECTURE.md` matches the
actual module tree with no undocumented modules; `cargo doc --no-deps` produces
zero `missing_docs` warnings.

---

## Phase 4 — Multi-Crate Workspace

**Goal:** split the single-crate monolith into focused library crates with
enforced dependency boundaries. This is the structural prerequisite for the policy
DSL in Phase 5 and the sandboxing layer in Phase 6.

### 4.1 Crate extraction order

Extract in this order — each crate must compile and pass its tests before the
next extraction begins:

```
aegis/                          (workspace root)
  crates/
    aegis-types/   [DONE]       RiskLevel, Decision, Pattern, Assessment + policy enums — no Aegis-crate deps (serde + schemars only)
    aegis-parser/  [DONE]       command tokenizer, PrefixPattern matching — depends on aegis-types
    aegis-scanner/ [DONE]       Scanner, PatternSet, PrefixRule — depends on aegis-types, aegis-parser
    aegis-policy/  [DONE]       PolicyEngine — depends on aegis-types, aegis-scanner (PrefixRule lives in aegis-scanner; amend in aegis-config)
    aegis-config/  [DONE]       AegisConfig, loader, validation, schema, amend — depends on aegis-types, aegis-scanner
    aegis-explanation/[DONE]     CommandExplanation and related types — depends on aegis-types, aegis-policy, aegis-config
    aegis-tui/     [DONE]       crossterm confirmation dialog — depends on aegis-types, aegis-explanation
    aegis-audit/   [DONE]       AuditLogger, AuditEntry — depends on aegis-types, aegis-scanner, aegis-config, aegis-explanation, aegis-policy
    aegis-snapshot/[DONE]       SnapshotPlugin trait + 6 backends — depends on aegis-types, aegis-config
    aegis-starlark/[DONE]       Starlark policy DSL loader (opt-in `starlark-policy`) — depends on aegis-types
    aegis-sandbox/ [DONE]       bwrap + Landlock (Linux) / sandbox-exec (macOS) execution confinement
    aegis-language/[DONE]       Tree-sitter runtime + grammars, parse-only worker, per-language adapters (added by milestone L1)
  src/                          binary — thin wiring, depends on all crates above
```

Status: 12 crates extracted — the nine above plus `aegis-starlark` in Phase 5,
`aegis-sandbox` in Phase 6, and `aegis-language` in milestone L1. `Cargo.toml` is
the authority on the workspace member list. `aegis-starlark` leaves the tree with
[#225](https://github.com/IliasAlmerekov/aegis-shellguard/issues/225)
([ADR-028](docs/adr/adr-028-the-starlark-policy-dsl-is-removed-before-1-0.md)).

Each `crates/X/Cargo.toml` must not depend on `aegis-binary` or any other
application crate. Dependency arrows flow inward toward `aegis-types`.

**Progress:** `aegis-types` extracted — the full data vocabulary: `RiskLevel`,
`Decision`, `Pattern`, `Category`, `PatternSource`, `PatternToken`,
`PrefixPattern`, and `Assessment` together with its embedded data types
(`MatchResult`, `DecisionSource`, `HighlightRange`, `ParsedCommand`,
`InlineScript`) plus their pure `Display`/`FromStr`/`decision_source` impls. The
scanning *logic* (`Scanner`, highlighting, helpers) stays in the root crate and
consumes these types. Definitions are re-exported from their original module
paths so call sites are unchanged.

`aegis-parser` extracted next — the shell tokenizer (`Parser`, `split_tokens`,
`extract_prefix`, segmentation, heredoc/inline-script/nested-shell extraction)
and the token-level `matches_prefix` matcher. `PrefixRule::matches_tokens` (now
in `aegis-scanner`) delegates to `aegis_parser::matches_prefix`.
`src/interceptor/parser/` is now a re-export shim.

`aegis-scanner` extracted next — `Scanner`, `PatternSet`, `PrefixRule`, the
recursive/nested-scan helpers, and the embedded `patterns.toml`. It exposes a
typed `ScannerError` (thiserror) instead of leaking the binary's `AegisError`;
the root maps `ScannerError → AegisError` at the orchestration boundary. The
`UserPattern → Pattern` conversion lives at the config/root boundary so the
scanner never sees config types. `src/interceptor/{scanner,patterns}.rs` are now
re-export shims.

`aegis-policy` extracted next — the pure `PolicyEngine` (`src/decision/`), which
maps an `Assessment` plus mode/CI/allowlist context to a `PolicyDecision`
(infallible, no error type). Prep moved the policy-config enums (`Mode`,
`CiPolicy`, `SnapshotPolicy`, `AllowlistOverrideLevel`) into `aegis-types` so
policy needn't depend on config. `PrefixRule` stays in `aegis-scanner` (DAG
direction) and `amend` (config-coupled decision persistence) is deferred to
`aegis-config`. `src/decision/` is now a re-export shim.

`aegis-config` extracted next — the config model, layered loader, validation,
JSON schema, and `amend` (decision persistence). Depends on `aegis-types` +
`aegis-scanner` (it validates that custom patterns compile via
`PatternSet`/`Scanner`). Prep introduced a typed `ConfigError` (replacing the
binary's `AegisError`) and replaced the binary's `interceptor::scanner_for`
validation with a config-local helper. The policy-config enums (`Mode`,
`CiPolicy`, `SnapshotPolicy`, `AllowlistOverrideLevel`) live in `aegis-types`;
`AuditConfig`/`AuditIntegrityMode` stay in `aegis-config`; `aegis-audit`
depends on it for them. `src/config/` is now a re-export shim.

### 4.2 Dependency rule enforcement via workspace architecture test

The dependency DAG is enforced by a Rust workspace architecture test
(`tests/architecture_boundaries.rs`). Each test reads the relevant workspace
member's `Cargo.toml` directly and asserts that `aegis-parser`, `aegis-scanner`,
and `aegis-types` do not list `aegis-audit`, `aegis-config`, `aegis-explanation`,
`aegis-tui`, or `aegis-snapshot` in their `[dependencies]` section.

Note: `cargo-deny` `[[bans.deny]]` with `wrappers` cannot express per-directed-edge
restrictions (the `wrappers` field means "banned except when pulled in transitively
by these crates", not "only these crates may depend on it"). The workspace
architecture test is the correct mechanism for enforcing DAG boundary rules.

CI fails if any crate violates the dependency boundary.

### 4.3 `aegis-parser` becomes a fuzz target

With the parser in its own crate, `fuzz/` can target it directly. Increase CI
fuzz iterations from 2000 to 100 000. Add the corpus from production runs
(sanitized command strings) to the fuzz corpus directory.

**Done when:** `cargo build --workspace` succeeds; `cargo test --workspace` passes;
a PR that adds a dependency from `aegis-parser` to `aegis-audit` fails CI via
the workspace architecture test in `tests/architecture_boundaries.rs`.

---

## Phase 5 — Policy DSL

**Goal:** replace TOML pattern tables with a typed, programmable policy language.
Users can express rules that require conditional logic, environment context, or
programmatic construction — without modifying Rust source. Inspired by codex's
Starlark-based `execpolicy` parser.

### 5.1 Evaluate DSL options

Before committing to an implementation, benchmark three approaches against Aegis's
2 ms hot-path constraint:

| Option          | Expressiveness | Binary size impact | Startup cost |
| --------------- | -------------- | ------------------ | ------------ |
| Starlark (rhaï) | High           | +3–5 MB            | ~1 ms warmup |
| Lua (mlua)      | Medium         | +1–2 MB            | < 0.5 ms     |
| Typed TOML DSL  | Low–Medium     | Zero               | Zero         |

Recommended starting point: **typed TOML DSL** (§5.2 below), with Starlark/Lua as
an opt-in power-user feature (§5.3). The typed DSL covers 95% of real use cases
without embedding an interpreter.

That opt-in tier is **withdrawn**: the Starlark DSL is deleted from the tree
before 1.0, and the typed TOML DSL is the only way to declare a Policy rule
([ADR-028](docs/adr/adr-028-the-starlark-policy-dsl-is-removed-before-1-0.md),
[#225](https://github.com/IliasAlmerekov/aegis-shellguard/issues/225)).
`PRD.md` §5.2 is the normative statement.

### 5.2 Typed TOML policy DSL (Done)

Extend `aegis.toml` with a richer rule type:

```toml
[[rules]]
pattern     = ["git", "push", ["--force", "-f"]]
decision    = "prompt"
justification = "Force-push rewrites remote history."
match_examples     = ["git push --force origin main"]
not_match_examples = ["git push origin main"]

[[rules]]
pattern  = ["rm", "-rf", "/"]
decision = "block"

[[rules]]
pattern  = ["docker", "run"]
decision = "prompt"
when     = { env = "CI", value = "true", then = "allow" }
```

The `when` clause adds environment-conditional decisions. The rule is validated
at load time (not at match time) — invalid rules are a startup error.

### 5.3 Starlark policy DSL (power-user tier) (Done)

For users who need programmatic rules, offer an opt-in Starlark policy file
(`~/.aegis/policy.star`):

```python
prefix_rule(
    pattern = ["kubectl", "delete"],
    decision = "prompt",
    justification = "Deleting Kubernetes resources is irreversible.",
    match_examples = [["kubectl", "delete", "pod", "mypod"]],
)

def on_command(cmd):
    if cmd[0] == "git" and "--force" in cmd:
        return "prompt"
    return "allow"
```

Starlark is evaluated at startup and the resulting rule set is compiled to the
same `MultiMap<program, PrefixRule>` used by the typed DSL. There is no runtime
Starlark evaluation on the hot path.

**Done when:** a user can write `~/.aegis/aegis.toml` with `[[rules]]` entries
using `Alts`, `when`, `justification`, and `match_examples`; invalid rules produce
a human-readable error with line numbers; the hot path shows no regression on
`cargo criterion`.

---

## Phase 6 — Sandboxing Layer

**Goal as written:** add an optional best-effort write/network guardrail around
approved commands. This Sandbox is not a confidentiality boundary and does not
replace Aegis' heuristic decision guardrail.

**Superseded on the "optional" half by
[ADR-029](docs/adr/adr-029-the-sandbox-is-a-mandatory-1-0-layer.md).** The 1.0
contract, stated normatively in `PRD.md` §5.5, is:

- Confinement is a **mandatory 1.0 layer**. It is attempted for every executed
  command outside `Mode::Audit`, and failure to establish it **blocks execution**
  rather than downgrading to an unconfined run.
- `sandbox.enabled` and `sandbox.required` leave the contract: they are accepted
  by exact name and **ignored at any value**, with a typed
  `deprecated_sandbox_field` diagnostic, for the whole support life of config
  schema v1. Aegis never rewrites a config file to remove them.
- Write and network authority is bounded by the `Trusted ceiling` and narrowed
  further by whichever rules matched the command
  ([ADR-030](docs/adr/adr-030-the-confinement-profile-is-derived-from-the-assessment.md)).
- The surviving honesty claim is unchanged: the layer is a write/network
  guardrail, **not a confidentiality boundary and not a privilege boundary**.

**Current pre-1.0 implementation (0.6.x):** the shipped code still implements the
optional model described in the subsections below — `sandbox.enabled` constructs
the layer, unavailability warns and proceeds unconfined, and
`sandbox.required = true` is what turns that into a block. The subsections are
kept as the record of what was built. The gap closes with
[#229](https://github.com/IliasAlmerekov/aegis-shellguard/issues/229) and
[#230](https://github.com/IliasAlmerekov/aegis-shellguard/issues/230); until then
no document may claim the layer is already mandatory.

### 6.1 Linux — bubblewrap + Landlock

- `bwrap` (bubblewrap): namespace-based sandbox. Approved commands run in a
  new mount namespace with a read-only view of the filesystem except for
  explicitly allowed write paths.
- `landlock`: Linux Security Module for fine-grained filesystem access control.
  Applied in addition to bwrap for defense in depth.

```toml
# Current pre-1.0 implementation. `enabled` leaves the contract in 1.0 (ADR-029);
# `allow_write` becomes an explicit override of a computed `Trusted ceiling`
# default, and `allow_network` keeps its meaning. See `docs/config-schema.md`.
[sandbox]
enabled = true
allow_write = [".", "/tmp"]
allow_network = false
```

### 6.2 macOS — Seatbelt (`sandbox-exec`)

Apply a `.sbpl` sandbox profile via `/usr/bin/sandbox-exec` before exec'ing the
approved command. Profile templates live in `crates/aegis-sandbox/profiles/`.

### 6.3 Windows — Job Objects (withdrawn)

> **Withdrawn by the decision to drop native Windows.** No native Windows build ships;
> Windows is supported only through WSL2, which uses the Linux bwrap + Landlock
> path. This subsection is retained for historical context only.

Use Windows Job Objects to confine approved commands. The initial MVP delivers
`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` so all child processes are terminated when
Aegis exits. Full restrictions on subprocess creation, filesystem writes outside
allowed paths, and network access require AppContainers or WFP and are deferred
to a follow-up task.

### 6.4 Sandbox bypass is an audit event

If Sandbox infrastructure cannot be applied (kernel version too old, missing
capabilities, unsupported platform), Aegis records
`sandbox_status = "unavailable"`. In the **current pre-1.0 implementation** it
then warns on the active Shell stderr or Watch NDJSON channel and proceeds
unconfined, unless the user configured `sandbox.required = true` to turn
unavailability into a hard block. Invalid profiles and unexpected setup errors
fail closed rather than degrading.

Under the 1.0 contract there is no unconfined continuation to configure: the
recorded `sandbox_status = "unavailable"` accompanies `Decision::Blocked` as one
event, and the flag that used to select between the two is gone
([ADR-029](docs/adr/adr-029-the-sandbox-is-a-mandatory-1-0-layer.md)).

**Done when:** `cargo test --workspace` passes with sandbox enabled on
`ubuntu-latest` and `macos-latest`; a command that attempts to write outside the
allowed paths is killed by the sandbox; the audit log records the sandbox profile
applied for every executed command.

---

## Milestone L1 — Language-aware analysis (1.x)

**Goal:** add a bounded, production-qualified semantic slow path for source that
agents pass to interpreters, without replacing the shell Scanner or regressing
the no-source safe-command hot path.

**This milestone is not in 1.0.** It was charted as pre-1.0 and moved out by
[ADR-024](docs/adr/adr-024-language-aware-analysis-ships-opt-in-and-is-not-a-1-0-release-gate.md):
it ships after 1.0 behind the `language-analysis` cargo feature, default off, and
the Tree-sitter grammars are absent from the 1.0 release binaries. `PRD.md` §5.11
states it as an explicit 1.0 Non-Goal. The grounds are scope, not schedule —
finishing the qualification matrix early does not return it to 1.0. The gating
work is [#212](https://github.com/IliasAlmerekov/aegis-shellguard/issues/212) and
[#213](https://github.com/IliasAlmerekov/aegis-shellguard/issues/213).

**`L1` is defined here and nowhere else.** It names this milestone — the language-aware
analysis track — and carries no other meaning in the repository. In particular it is not
a distribution gate: Homebrew, npm, and installer smoke evidence belongs to the
distribution gates in [`docs/release-readiness.md`](docs/release-readiness.md) and never
blocks `L1`. Milestone IDs and finding IDs are disjoint namespaces (`CONVENTION.md` §11).

Architecture and trade-offs are fixed by
[`ADR-022`](docs/adr/adr-022-language-aware-analysis-is-an-additive-isolated-stage.md).
The test-first delivery sequence is in
[`docs/plans/2026-07-16-language-aware-analysis.md`](docs/plans/2026-07-16-language-aware-analysis.md).

### L1.1 Shared foundation

- [ ] Common Detection rule, typed Match evidence, Detected operation,
      Assessment basis, Analysis provenance, and typed degradation model.
- [ ] Ephemeral self-spawned parsing worker with a versioned, bounded pipe
      protocol and no worker filesystem/subprocess access.
- [ ] Async catch-only Script source inspection, heredoc/stdin routing, bounded
      cwd tracking, and recursive cross-language target queue.
- [ ] Audit schema v2 compatibility, consolidated confirmation UX, Policy/CI
      integration, config ratchets, privacy tests, fuzzing, and benchmarks.
- [ ] Pinned Tree-sitter runtime and grammar manifest pass license,
      supply-chain, ABI, corpus, and all-target release qualification.

### L1.2 First adapters

`Default-on` here means default-on **once the `language-analysis` feature is
enabled**, not on by default in a 1.0 build — the feature itself is off (ADR-024).

- [ ] Python is production-qualified and default-on.
- [ ] JavaScript is production-qualified and default-on.
- [ ] TypeScript is production-qualified and default-on.
- [ ] Shell/Bash is production-qualified and default-on.

### L1.3 Staged 1.x coverage

Go, PHP, Ruby, PowerShell, Perl, and Lua remain explicit 1.x adapters. Each stays
unsupported until it independently passes the same grammar, semantics, worker,
privacy, fuzz, benchmark, audit, interface, and four-target release gates. There
is no big-bang enablement and no runtime grammar download fallback.

**Done when:** the shared foundation and all four first adapters pass the
qualification matrix, official binaries contain the same pinned grammar set on
both Linux musl and both macOS architectures, the no-source safe path remains
under 2 ms without starting a worker, and the language-analysis re-entry
conditions in `docs/release-readiness.md` are complete. Those conditions govern
enabling it in a 1.x release; none of them blocks 1.0.

---

## Phase 7 — Release Readiness

**Goal:** ship a 1.0 release.

This phase carries **no checklist**. What must be true for 1.0 is promised
normatively in [`PRD.md`](PRD.md) §5–§8, and what still blocks it is the
[`1.0` milestone](https://github.com/IliasAlmerekov/aegis-shellguard/milestone/1) —
the only gate
([ADR-027](docs/adr/adr-027-one-1-0-release-gate-lives-in-the-issue-tracker.md)).
The list that used to stand here was one of three mutually inconsistent 1.0 gates
in this repository; that is exactly the drift a single gate removes.

`docs/release-readiness.md` remains the place where **evidence** of completed
release work is recorded — installer runs, checksum verification, tap and npm
smoke tests, supply-chain gate output. Evidence is not a gate: a checked box there
records that something was verified, never that it is required.

Platform scope for the release is `docs/platform-support.md`: Linux
x86_64/aarch64 and macOS arm64/x86_64, with Windows through WSL2 only — native
Windows was dropped, and `PRD.md` §8 is the normative statement.

---

## Summary

| Phase | Name                  | Key deliverable                                    |
| ----- | --------------------- | -------------------------------------------------- |
| 0     | Foundation Repair     | No silent failures; async correct; MSRV declared    |
| 1     | Scanner Modernization | Token-prefix matching; `justification` in TUI      |
| 2     | Decision Persistence  | "Always allow/block" writes rules to config        |
| 3     | Module Architecture   | No file > 800 lines; typed `AuditEntry`; live docs |
| 4     | Multi-Crate Workspace | 9 core crates (12 total); enforced dependency DAG   |
| 5     | Policy DSL            | Typed TOML rules (the only authoring path in 1.0)  |
| 6     | Sandboxing Layer      | bwrap/Landlock/Seatbelt on executed commands       |
| L1    | Language-aware analysis | Foundation + Python, JavaScript, TypeScript, Bash |
| 7     | Release Readiness     | 1.0 ships                                          |
