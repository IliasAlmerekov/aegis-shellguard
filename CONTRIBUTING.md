# Contributing

Thanks for helping improve Aegis.

Before opening a pull request, please read:

- [`CONVENTION.md`](CONVENTION.md) — project rules, security invariants, style, and release gates
- [`docs/adr/README.md`](docs/adr/README.md) — architecture decision records and documented non-goals
- [`SECURITY.md`](SECURITY.md) — responsible disclosure process for security reports

For non-trivial changes, please open an issue first so we can agree on scope before implementation.

## What kinds of pull requests are welcome

Good fits for this repository:

- bug fixes with focused regression coverage
- tests that improve confidence in parser, scanner, policy, snapshot, or audit behavior
- documentation improvements that make the security model or contributor workflow clearer
- targeted performance improvements with benchmark evidence
- small UX improvements that do not weaken approval or audit guarantees

Usually not a good fit without prior discussion:

- drive-by dependency swaps
- broad refactors with no user-visible benefit
- changes that weaken fail-closed, approval, snapshot, or audit behavior
- CI / release-policy changes without an agreed issue or maintainer request
- roadmap-sized features submitted as a surprise PR

## Development environment

Minimum local setup:

- Rust 1.89 or newer, the minimum supported Rust version (MSRV) in `Cargo.toml`
- Rust 1.94.0 to run `scripts/lint.sh`, the toolchain CI pins in `.github/versions.env`
- Git
- a Unix-like environment supported by the project (Linux or macOS)
- on Linux, the `libcap` headers: `sudo apt-get install -y libcap-dev` on Debian and Ubuntu, or your distribution's equivalent

Without the `libcap` headers the Linux build fails by design, with
`failed to compile bubblewrap for Linux target: libcap not available via pkg-config`.
[`docs/troubleshooting.md`](docs/troubleshooting.md) describes the local-only
`AEGIS_SKIP_BWRAP_BUILD` and `AEGIS_BWRAP_SOURCE_DIR` overrides.

Optional but useful:

- `cargo-audit` for local advisory checks
- `cargo-deny` for local dependency-policy checks
- the pinned nightly Rust plus `cargo-fuzz` for fuzzing (`FUZZ_NIGHTLY_TOOLCHAIN` in `.github/versions.env`)
- Docker if you want to opt in to the real Docker integration tests

Install helper tools if you want the full local verification surface:

```sh
cargo install cargo-audit cargo-deny cargo-fuzz
rustup toolchain install 1.94.0 --component clippy,rustfmt
rustup toolchain install nightly-2026-06-15
```

## Build the project

From the repository root:

```sh
cargo build
```

For a release build:

```sh
cargo build --release
```

## Run tests

The repository is a Cargo workspace with a root package and twelve member
crates under `crates/`. A bare `cargo test` runs only the root package. CI
runs the full suite with `cargo test --workspace`. Locally, run only the test
for the area you changed:

```sh
cargo test --test full_pipeline_policy
```

The end-to-end tests live in eight targets, one per area:
`full_pipeline_allowlist`, `full_pipeline_audit`, `full_pipeline_config`,
`full_pipeline_json`, `full_pipeline_policy`, `full_pipeline_shell`,
`full_pipeline_snapshot` and `full_pipeline_toggle`. To run the tests of one
member crate, use `cargo test -p aegis-parser`, for example.

Docker integration tests are skipped by default. To opt in:

```sh
AEGIS_DOCKER_TESTS=1 cargo test --test docker_integration
```

These tests require a working Docker daemon and `docker` on `PATH`.

## Run formatting and linting

```sh
scripts/lint.sh
```

The script runs rustfmt and clippy, over every target and feature, on the
Rust version pinned in `.github/versions.env`. These are the same commands CI
runs. If that version is missing, the script prints the `rustup` command that
installs it. Pass `fmt` or `clippy` to run one of the two.

## Run benchmarks

If your change touches parser/scanner hot paths or benchmark-sensitive behavior:

```sh
cargo bench --bench scanner_bench
```

See [`docs/performance-baseline.md`](docs/performance-baseline.md) for the benchmark policy and local interpretation guidance.

## Run fuzzing

The repository includes nine fuzz targets in `fuzz/fuzz_targets/`: `parser`,
`scanner`, `heredoc`, `router`, `language_protocol`, `language_python`,
`language_javascript`, `language_typescript` and `language_bash`. List them
and run one with the pinned nightly Rust:

```sh
cargo +nightly-2026-06-15 fuzz list
cargo +nightly-2026-06-15 fuzz run parser fuzz/corpus/parser
```

Fuzzing guidance and current status are documented in [`docs/adr/README.md#verification-guidance`](docs/adr/README.md#verification-guidance).

## What CI checks

CI runs every gate on each pull request, and all of them must pass to merge:

- `scripts/lint.sh` (rustfmt and clippy on the pinned toolchain)
- `cargo test --workspace`, which covers the root package and every member crate
- `cargo audit` and `cargo deny check`
- a regenerated `aegis-schema.json`, which must match the committed file

You do not need to run these locally. A full local build takes tens of
gigabytes of `target/`. Run the targeted test for the code you changed, and
let CI run the rest.

The one local check is formatting. Install the repository-managed Git hook once
per clone:

```sh
./scripts/setup-git-hooks.sh
```

The pre-push hook runs `scripts/lint.sh fmt`, which compiles nothing, and a
formatting diff blocks the push. After you change the config model in
`crates/aegis-config`, run `cargo run --bin aegis_schema` from the repository
root and commit the updated `aegis-schema.json`, or the CI schema check fails.

## Pull request checklist

Please make sure your PR:

- has a clear summary of what changed and why
- includes tests or explains why no test change was needed
- keeps documentation in sync with behavior
- does not present the Sandbox as a confidentiality or privilege boundary, or
  Aegis as a complete security boundary (ADR-029)
- stays focused; unrelated cleanup should be split into a separate PR

## Where to ask questions

- Bugs and actionable work items: GitHub Issues
- Security reports: follow [`SECURITY.md`](SECURITY.md), not public issues
- Roadmap / idea discussion: GitHub Discussions, once enabled in repository settings
