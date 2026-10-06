# CI and Release Guarantees

## Pinned Inputs

Every version below is written once, in `.github/versions.env`. Both workflows
read that file through the `.github/actions/load-versions` composite action and
re-export it as job outputs, so a bump is a one-line change that reaches every
job in both workflows. `tests/supply_chain_ci.rs` fails when a workflow writes
a version a second time, and when this list disagrees with the file. It also
fails when `CONTRIBUTING.md` names a Rust toolchain other than
`RUST_TOOLCHAIN` or `FUZZ_NIGHTLY_TOOLCHAIN`, so a toolchain bump updates
`CONTRIBUTING.md` too.

- Rust toolchain: `1.94.0`
- `cargo-audit`: `0.22.1`
- `cargo-deny`: `0.19.0`
- `cross`: `0.2.5`
- `cargo-llvm-cov`: `0.9.1` and `cargo-mutants`: `27.1.0`, for the informational
  jobs in `.github/workflows/pipeline.yml`
- npm CLI for trusted publishing: `11.13.0`
- The release target matrix and every runner label live in
  `.github/build-targets.json`. `ci.yml` (`Cross build`) and `release.yml`
  (`build`) both expand it into `strategy.matrix.include`, and the jobs that
  name a runner outside the matrix read their label from the same file. Linux
  runs on `ubuntu-24.04` and macOS on `macos-26`; no job uses a floating
  `*-latest` label. The `gate` job in `ci.yml` and the `config` job in
  `release.yml` load the labels, so they repeat the Linux label as a literal.
  So does the Merge admission check, which must run even when `gate` failed.
  A runner-image bump edits `.github/build-targets.json` and those three lines,
  and `tests/supply_chain_ci.rs` fails if they disagree.
- GitHub Actions used by `.github/workflows/ci.yml` and `.github/workflows/release.yml` are pinned by full commit SHA with readable release comments. `.github/dependabot.yml` opens the weekly pull request that moves those SHAs forward, along with the cargo and npm dependency updates.

## Current CI Jobs

`ci.yml` runs on a pull request, a push to `main`, a `merge_group` event, the
weekly schedule, and `workflow_dispatch`. The table shows which jobs run for
each event. A Heavy job is one behind the `heavy` output of the gate job.

| Job | PR into `main` | PR into another branch | Push to `main` | `merge_group` | `schedule` / `workflow_dispatch` |
| --- | --- | --- | --- | --- | --- |
| `Determine heavy-job gate` | yes | yes | yes | yes | yes |
| `Quality (fmt, clippy, test)` | yes | yes | yes | yes | yes |
| `Landing (test, build)` | yes | yes | yes | yes | yes |
| `Security (audit, deny)` | yes | yes | yes | yes | yes |
| `Release build (ubuntu-24.04)` | yes | yes | yes | yes | yes |
| `Release build (macos-26)` (Heavy job) | yes | no | yes | yes | yes |
| `Cross build` (Heavy job) | yes | no | yes | yes | yes |
| `Performance baseline (scanner bench)` (Heavy job) | yes | no | yes | yes | yes |
| `Live installer validation` (Heavy job) | yes | no | yes | yes | yes |
| `Live snapshot/rollback (Docker + SQLite)` (Heavy job) | yes | no | yes | yes | yes |
| `Live snapshot/rollback (SQLite, macOS)` (Heavy job) | yes | no | yes | yes | yes |
| `Fuzzing (parser, scanner, routing, protocol, adapters)` (Heavy job) | yes | no | yes | yes | yes |
| `Merge admission (all CI jobs)` | yes | yes | yes | yes | yes |

What each job runs:

- `Determine heavy-job gate`: loads the pinned versions and runner labels, and computes `heavy`
- `Quality (fmt, clippy, test)`: formatting, clippy, and tests
- `Landing (test, build)`: type check, tests, and static export build of the `landing/` npm workspace
- `Security (audit, deny)`: `cargo-audit` and `cargo-deny`
- `Release build (ubuntu-24.04)` and `Release build (macos-26)`: `cargo build --release` on each runner. The Linux label in the name comes from `.github/build-targets.json`.
- `Cross build`: the four-target cross matrix builds the shipping release binary so qualified grammars are linked into every release artifact
- `Performance baseline (scanner bench)`: `scanner_bench` plus benchmark policy evaluation
- `Live installer validation`: downloads the latest GitHub Release asset for the host platform, verifies the SHA-256 sidecar, installs to a temporary `BINDIR`, and asserts `aegis --version` succeeds. Runs on the pinned Linux and macOS runners (`ubuntu-24.04` and `macos-26`); gated in the test suite by `AEGIS_TEST_LIVE_INSTALL=1` so default `cargo test` stays network-free.
- `Live snapshot/rollback (Docker + SQLite)`: runs on the pinned Linux runner, pulls the real `alpine` Docker fixture image, installs the real `sqlite3` CLI, then runs the gated Docker and SQLite snapshot/rollback integration tests with `AEGIS_DOCKER_TESTS=1` and `AEGIS_SQLITE_SNAPSHOT_TESTS=1`.
- `Live snapshot/rollback (SQLite, macOS)`: runs on the pinned macOS runner and executes the same gated SQLite lifecycle test as the Linux job (`AEGIS_SQLITE_SNAPSHOT_TESTS=1`) against the `/usr/bin/sqlite3` the OS ships. It skips the Docker test: hosted macOS runners have no Docker daemon preinstalled, so `docker pull alpine` and `docker_integration` cannot run there. Docker snapshot and rollback therefore stay untested on macOS in CI; only Linux covers the Docker plugin against a live daemon. Revisit if a hosted macOS runner gains a Docker daemon (#411).
- `Fuzzing (parser, scanner, routing, protocol, adapters)`: corpus-backed
  parser, scanner, heredoc, router, language-protocol, Python, JavaScript,
  TypeScript, and Bash fuzz targets with bounded `-runs`, on the pinned
  `FUZZ_NIGHTLY_TOOLCHAIN` nightly rather than a floating one
- `Merge admission (all CI jobs)`: the Merge admission check, described below

### How `heavy` is computed

The gate job sets `heavy=true` for a pull request whose base is `main`, a push
to `main`, a `merge_group` event, the weekly schedule, and `workflow_dispatch`.
It sets `heavy=false` for a pull request into any other branch. Heavy jobs
carry `if: needs.gate.outputs.heavy == 'true'`, so they are skipped when
`heavy` is `false`.

### Merge admission check

The Merge admission check is the job `Merge admission (all CI jobs)`. It needs
every other job in `ci.yml` and runs with `if: always()`, so a failed job cannot
skip it. It fails when any job failed or was cancelled. When `heavy` is `true`,
it also fails on any job that did not succeed. When `heavy` is `false`, a Heavy
job may be skipped, and every other job must succeed. An empty `heavy` value
means the gate job itself failed, and the check fails.

The job is the only required status context on `main`, with "require branches
to be up to date" on. Its `name:` must stay unchanged, because branch
protection requires it by name. A test fails when a job is added to `ci.yml`
without being listed in its `needs:`. Because the check needs every job, the macOS live snapshot/rollback job blocks merge like the rest. If that job turns flaky and blocks every PR, the rollback is to delete the job, drop it from the `needs:` list and `HEAVY_JOBS`, and remove its assertions in `tests/snapshot_rollback_ci.rs` in the same change; the Linux job keeps the SQLite coverage.

### Concurrency

On `main`, the concurrency group is keyed by the commit SHA and runs are never
cancelled, so every commit that lands on `main` gets its own full run. GitHub
cancels a pending run when a newer run joins the same group, and a SHA-keyed
group avoids that. On every other ref, the group is keyed by the ref and a
newer run cancels the older one.

### Release workflow jobs

`release.yml` runs these jobs:

- `Release / Tag admission (commit on main, CHANGELOG section)`,
  `Release / Tag admission (fmt, clippy, test)`,
  `Release / Tag admission (audit, deny)`: the Tag admission check, described
  under [Release Workflow Contract](#release-workflow-contract). The two Rust
  jobs run the same `quality-gate` and `security-gate` composite actions as
  `Quality (fmt, clippy, test)` and `Security (audit, deny)` above.
- `Release / build`: tagged release binaries for:
  - `x86_64-unknown-linux-musl`
  - `aarch64-unknown-linux-musl`
  - `x86_64-apple-darwin`
  - `aarch64-apple-darwin`
- `Release / release`: artifact download plus GitHub Release publication

## Agent pipeline workflow

`.github/workflows/pipeline.yml` runs on every pull request and is
informational. Its jobs are not in the Merge admission check and are not
required status checks.

- `Pipeline scope (packages, tier)`: maps the changed files to cargo packages
  and to the highest review tier, with the rules in `.agents/pipeline.json`
- `Test tamper check`: `scripts/test-tamper-check.sh` against the merge-base
  with the PR's base branch. It fails on added skip markers or lost assertions
  unless the PR head commit body carries `tamper-check: allow <reason>`
- `Diff coverage (changed packages)`: `cargo llvm-cov` for the changed packages,
  then the share of added Rust lines that ran. Below 80% it writes a warning;
  it never fails. `lcov.info` and `diff-coverage.md` are uploaded as artifacts
- `Mutants (changed code)`: `cargo mutants --in-diff` on the changed packages,
  only for TRUNK and BRANCH tier PRs. It cannot fail the run; `mutants.out` is
  uploaded as an artifact

## Homebrew tap validation

Homebrew validation is currently a release-operator smoke test rather than a
default CI job. The formula lives at `packaging/homebrew/Formula/aegis.rb` and is
regenerated from a release tag by `scripts/update-homebrew-formula.sh`. The
required `brew tap` / `brew install` / `brew test` commands on macOS and Linux
are listed in `docs/release-readiness.md`. A gated live test
(`AEGIS_TEST_LIVE_HOMEBREW=1`) lives in `tests/homebrew_live.rs` and keeps
default `cargo test` network-free; a CI job that runs it on
the pinned Linux and macOS runners will be added only after explicit workflow
sign-off.

## npm package validation

npm package validation is a release-operator smoke test until explicit CI
workflow sign-off is granted. Network-free contract tests live in
`tests/npm_package.rs` and assert the manifest, installer fail-closed
behavior, checksums, updater, and docs without touching the network. The gated
live test in `tests/npm_live.rs` runs only when `AEGIS_TEST_LIVE_NPM=1` is set,
keeping default `cargo test` network-free. The npm wrapper downloads a pinned
GitHub Release binary during `postinstall`, verifies SHA256, and does not edit
shell startup files or agent config.

The release workflow publishes npm with GitHub OIDC, not `NPM_TOKEN`. Its
`publish-npm` job requests `id-token: write`, uses pinned Node and npm CLI
versions, and reads checksums from the published GitHub Release. If that job
fails after a tag release succeeds, an operator can dispatch `release.yml` on
`main` with the existing stable tag. The recovery run verifies the tag is on
`main` and the GitHub Release is public, checks out the tagged source, then
repeats only npm package preparation and publishing. It does not rebuild or
replace the GitHub Release. A recovery dispatch omits npm provenance because
its workflow commit on `main` differs from the tagged package source. Normal
tag-triggered publishes retain provenance. The npm package must trust
`release.yml` as a GitHub Actions publisher and allow `npm publish`.

## What CI Guarantees

- the workflow definitions do not depend on floating toolchain, tool, or action refs
- `ci.yml` runs formatting, linting, tests, dependency audit, deny policy checks, release builds, and benchmark policy checks exactly as defined in the pinned workflows
- `release.yml` re-runs formatting, linting, tests, dependency audit, and deny policy checks against the tagged commit before it builds anything, using the same composite actions `ci.yml` uses (see the Tag admission check below)
- CI additionally verifies parser, scanner, heredoc, routing, language-protocol,
  and qualified-adapter fuzzing with bounded corpus-backed runs.
- release artifacts are checksumed and uploaded by the pinned release workflow
- CI exercises snapshot and rollback behavior against live Docker and SQLite backends in the dedicated live snapshot/rollback job.

## What CI Does Not Guarantee

- byte-for-byte reproducible binaries across all environments
- independence from hosted-runner image changes, the crates ecosystem, or external infrastructure
- stronger runtime security semantics than the Aegis code actually implements
- that GitHub Actions CI and Aegis runtime CI handling are the same feature

## Runtime `ci_policy` vs GitHub Actions CI

These are different contracts:

- GitHub Actions CI is the repository automation defined in `.github/workflows/*.yml`
- runtime `ci_policy` is an Aegis config input that changes how the Aegis binary behaves when it detects CI

Current runtime behavior is documented in `docs/config-schema.md`, but at a high level:

- `ci_policy` is part of the runtime policy engine, not the workflow definition
- in `Protect`, `ci_policy = Block` blocks non-safe commands instead of prompting
- `Strict` is not weakened by CI detection
- `Audit` remains non-blocking

## Release Workflow Contract

On a push of a tag matching `v*`, the release workflow:

- runs the Tag admission check before building or publishing artifacts
- installs Rust `1.94.0`
- uses `cross 0.2.5` for both Linux musl targets (`x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`) so the release matrix does not depend on runner-specific musl linker setup
- builds the current four-target release matrix
- verifies Linux musl artifacts are statically linked with `readelf` by checking
  that the ELF headers contain no dynamic interpreter (`PT_INTERP`) and no
  shared library dependencies (`DT_NEEDED`) before checksum generation
- copies and renames the `aegis` binary per target asset name
- generates SHA-256 checksum sidecar files
- uploads artifacts from the build job
- publishes a GitHub Release with generated release notes and the built artifacts

A manual dispatch from `main` accepts an existing stable tag only for npm
recovery. It skips Tag admission, builds, and GitHub Release creation, and
publishes from the validated tagged commit.

### Tag admission check

`ci.yml` does not trigger on tags. Before this check existed, pushing a `v*`
tag went straight to the build matrix, so a tag on a commit that never went
through CI shipped binaries to the GitHub Release, npm, and the Homebrew tap.

The Tag admission check is the set of conditions the tagged commit must satisfy
before `release.yml` builds anything. Three jobs run it:

1. `Tag admission (commit on main, CHANGELOG section)` asks the GitHub compare
   API whether the tagged commit is reachable from `main`, accepting only the
   `identical` and `behind` statuses, and asserts `CHANGELOG.md` has a
   non-empty `## [<version>]` section for the tag. Reachability is what proves
   the commit passed review and the required status checks on `main`; no
   re-run of fmt or clippy can show that.
2. `Tag admission (fmt, clippy, test)` runs the `quality-gate` composite
   action: `scripts/lint.sh fmt`, `scripts/lint.sh clippy`, `cargo test
   --workspace`. The script runs rustfmt and clippy on the `RUST_TOOLCHAIN`
   pinned in `.github/versions.env`, over every target and feature.
3. `Tag admission (audit, deny)` runs the `security-gate` composite action:
   `cargo audit` and `cargo deny check`.

Jobs 2 and 3 need job 1 and run in parallel. The build matrix needs both of
them, so a failing condition stops the release before any artifact exists, and
therefore before any publish step.

Both Rust jobs call the same composite actions as the `Quality (fmt, clippy,
test)` and `Security (audit, deny)` jobs in `ci.yml`, so the tagged commit is
held to the identical standard rather than to a copy that can drift. The
actions are composite rather than reusable `workflow_call` workflows because a
called workflow renames its jobs' check runs, which would break the required
status contexts configured on `main`.

The check applies to prerelease tags (`-rc`, `-beta`, `-alpha`) in full. Those
binaries reach a public GitHub Release and get installed by hand.

`cargo audit` reads a moving advisory database, so job 3 can fail on a commit
that was green when it merged. That is intended: Aegis does not publish a
binary with a known CVE in its dependency chain. The way out is to update the
dependency, merge, and re-tag.

The Tag admission check is not the 1.0 release gate. That one is milestone
membership and lives in the issue tracker
([ADR-027](adr/adr-027-one-1-0-release-gate-lives-in-the-issue-tracker.md)).

This is a deterministic workflow-input contract, not a formal reproducible-build guarantee.

The release asset validation gate is covered by the gated
`tests/release_assets_live.rs` integration test. It is disabled during default
`rtk cargo test`; release operators run it with
`rtk env AEGIS_TEST_LIVE_RELEASE=1 AEGIS_TEST_RELEASE_TAG=vX.Y.Z cargo test
--test release_assets_live -- --nocapture` after a tag has been published to
prove that GitHub Release assets and `.sha256` sidecars are both present and
mutually consistent.

For users who want to validate a downloaded release asset before installing it,
see [Release readiness](release-readiness.md). That document also splits launch
blockers from longer-term security hardening items.
