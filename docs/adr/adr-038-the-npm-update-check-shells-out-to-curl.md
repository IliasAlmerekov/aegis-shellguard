# ADR-038: The npm update check shells out to `curl` and never self-updates

## Status

Accepted. Introduces `Update check`, `Update state`, `Update notice`, and
`Installation channel` to `CONTEXT.md`.

## Context

Users who installed `@iliasalmerekov/aegis` with npm have no signal that a
newer release exists — no built-in Aegis command has ever needed to make an
outbound network call, so Aegis has never linked an HTTP or TLS client.

The product decisions were settled before this ADR (see the design-interview
handoff): npm-only in v1, opt-in via `aegis update enable --channel npm`,
consent and cache state global under `~/.aegis/` and never in a project
`.aegis.toml`, notice rendering restricted to an interactive TTY, at most one
notice per day per distinct available version, and Aegis must never self-update
or execute `npm` — it only prints the version and the exact command to run.
What was left open was the Rust mechanism: how to make one bounded HTTPS
request without widening Aegis's own build or dependency surface, which is
security-sensitive precisely because Aegis is the tool auditing that surface
for everyone else.

## Decision

### 1. The registry fetch shells out to the system `curl`, not a Rust HTTP client

`CONVENTION.md` prohibits dependencies that bring in a native-C build step
beyond the two already-sanctioned ones (Tree-sitter in `aegis-language`,
vendored bubblewrap in `aegis-sandbox`). Any Rust HTTP client capable of HTTPS
needs a TLS backend, and the two viable ones for `rustls` (`ring`,
`aws-lc-rs`) both compile vendored C/assembly — that would be a third,
unscoped native-code build input pulled into the main `aegis` package (bin and
lib share one `Cargo.toml`) for a single opt-in GET request, plus roughly a
dozen transitive crates (`rustls`, `webpki-roots`, `http`, and friends) that
now sit in every `cargo audit` and `cargo deny` run whether or not a user ever
enables the feature.

`registry::fetch_latest_version` (`src/update/registry.rs`) instead spawns
`curl --silent --show-error --fail --location --max-time 5 --max-filesize
65536 -A <ua> <url>` as a plain `std::process::Command`. Every argument is a
fixed literal; no user input reaches the command line, so there is no
injection surface. `curl` owns TLS, certificate validation, and redirect
handling entirely — Aegis never re-implements any of it, and never sees a raw
socket. The registry response is parsed only far enough to read the
`"version"` string field (`registry::parse_latest_version`); nothing else in
the payload — readme text, dependency list, maintainer data — is read, kept,
or logged, and a response over the 64 KiB cap is rejected before parsing.

**The trade-off**: this makes the feature depend on a `curl` binary being on
`PATH`, which is not guaranteed on every system Aegis runs on. That is
accepted rather than worked around, because the feature's own contract is
opt-in and best-effort — "an unavailable network... must not block a command"
already covers a missing `curl` as one more unavailable-network case. A
missing `curl` makes `check_now` return `CheckOutcome::Failed`; the state file
is left untouched and nothing else in Aegis is affected. `curl` is already the
project's own installer transport (`README.md`'s curl-based install), so most
systems that can install Aegis this way already have it.

### 2. The shell wrapper spawns a detached OS child, not a Tokio task

`main()` calls `process::exit()` immediately after CLI dispatch returns
(`src/main.rs`), which kills every thread and task in the process — a Tokio
background task spawned during the wrapper's own invocation would be killed
before a network round-trip could complete. `maybe_spawn_background_check`
(`src/update/mod.rs`) instead re-execs the same binary as a genuine child
process with the hidden `--internal-update-check <channel>` flag, `stdin`,
`stdout`, and `stderr` all set to `Stdio::null()`, and never waits on it —
mirroring the existing `--internal-language-worker` short-circuit in
`main.rs`, which re-execs for the same reason (ADR-022 §2). The flag is
recognized before clap parsing and before the Tokio runtime is built, so the
child pays for neither.

A file lock at `~/.aegis/update.lock` (create-new, owner-only, reclaimed after
120 seconds of no modification) deduplicates concurrent triggers — several
shells opening at once each try to spawn a check, and only the one that wins
the lock actually does. The 120-second reclaim window exists so a killed or
crashed child cannot wedge future checks indefinitely; it is well above the
5-second `curl --max-time`, so a live child never loses the lock to a false
reclaim.

### 3. The notice is throttled per version, not per calendar day

`~/.aegis/update.json` (`UpdateState`) stores the last version a notice was
shown for and when, not a rolling "last notice shown" timestamp. A newer
version becomes available, the throttle resets immediately rather than
waiting out the previous version's 24-hour window — the product contract is
"at most once per day **for the same available version**", not "at most one
notice a day regardless of what changed."

### 4. Version comparison is strict SemVer and fails closed

`version::newer_version` (`src/update/version.rs`) uses the `semver` crate
(zero dependencies of its own, pure Rust, no build step) for both the
installed and the registry version. Either side failing to parse — a
malformed registry response, or, in principle, a non-SemVer local build —
returns `None`: no notice, no update recommended. Nothing downstream ever
sees a version comparison that could not be verified.

### 5. `aegis update check` runs regardless of stored consent

`consent` gates two things: whether the shell wrapper spawns the automatic
background check, and whether a notice is ever allowed to print. It does not
gate the explicit `aegis update check` command — a user or script invoking
that command by name is itself the authorization for that one request, the
same way `npm outdated` does not require a prior opt-in. This keeps the
command useful standalone (`aegis update check` before `enable`, to see what
is available first) without a surprising extra step.

### 6. The hot path is untouched

`maybe_offer_update_notice` is called once, from `run_shell_text_outcome`
(`src/shell_wrapper.rs`), after the wrapped command has already run — never
before or during `assess()`, and never on the JSON transport (`Watch`,
`--output json`, and `aegis hook` never call it at all, so they never
construct the check). The only unconditional cost on the text path is one
`is_ci_environment()` check and one `stdout().is_terminal()` syscall, both
independent of `Assessment budget` and negligible next to the multi-
millisecond process-spawn floor that already dominates every invocation
(`benches/startup_bench.rs`'s `startup_safe_command` exercises the JSON
transport, which this change never touches). No change to `scanner.rs` or
`parser.rs` accompanies this ADR, so no new benchmark baseline was recorded.

## Consequences

- No new native-code build input, and no new HTTP/TLS dependency tree; the
  only new crate is `semver` (pure Rust, no dependencies of its own).
- The feature is inert without a `curl` binary on `PATH` — accepted as a
  best-effort limitation consistent with the feature's own opt-in contract.
- The background child is a real OS process, not a same-process task, so it
  is observable in a process list for the ~5-second `--max-time` window; it
  never writes to stdout/stderr and exits without anyone waiting on it.
- `~/.aegis/update.lock` and `~/.aegis/update.json` are new files under the
  existing `~/.aegis/` state directory, owner-only (`0600`), atomic-write
  (temp file + rename), matching the pattern already used by
  `~/.aegis/disabled` (`src/toggle.rs`) and the audit log.
