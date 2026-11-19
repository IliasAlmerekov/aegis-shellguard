# Verification

How an agent proves a change in this repo. `implement` reads this file to pick and run the proof for each acceptance criterion; `pr` reads the manual checks. `CONVENTION.md` §9 holds the testing rules; this file only says how to run things.

## Tests

- **Location**: unit tests sit in `#[cfg(test)]` modules next to the code, often as sibling `*_tests.rs` files or a `tests/` submodule. Integration tests live in `tests/` for the root `aegis` crate and in `crates/<name>/tests/` for each workspace crate. Fuzz targets live in `fuzz/fuzz_targets/`, benches in `benches/` and `crates/aegis-language/benches/`.
- **Single file**: `cargo test --test <file_stem>` for a root integration test file, `cargo test -p <crate> --test <file_stem>` for a crate's integration test, `cargo test -p <crate> <module::path>` for a unit test module.
- **Full suite**: CI only. It runs `scripts/lint.sh`, `cargo test --workspace`, `cargo audit` and `cargo deny check`. Locally run `scripts/lint.sh fmt` and the targeted test; a full local build costs tens of gigabytes of `target/` per checkout.
- **Property testing**: none installed. Generated-input coverage comes from the `cargo fuzz` targets (`libfuzzer-sys`) in `fuzz/`.

## Run

- **Start**: `cargo build --bin aegis`. There is no service; every invocation is one process.
- **Ready when**: `target/debug/aegis --version` exits 0.
- **Drive**: `cli` for verdicts: `target/debug/aegis -c '<command>'`, checking stdout, stderr and the exit code; `--output json` gives the evaluation as JSON. `tmux` for the interactive confirmation prompt, with `tmux send-keys` and `tmux capture-pane`.
- **URL or entry point**: `target/debug/aegis`.
- **Test login**: none.
- **Setup**: run every probe in a throwaway directory with its own `HOME`, so it never reads or writes the real `~/.aegis`. Set `AEGIS_AUDIT_PATH` to a file inside that directory. Unset `GIT_DIR`, `GIT_WORK_TREE` and `GIT_INDEX_FILE`, which git hooks export in linked worktrees. `AEGIS_CI=1` forces CI behaviour, `AEGIS_FORCE_NO_TTY=1` forces the no-TTY path.
- **Logs**: `target/debug/aegis audit` with the same `HOME` and `AEGIS_AUDIT_PATH`.
- **Stop**: none.

## Manual checks

What a human confirms before merge, and when each applies.

- Run Aegis on a real Mac after any change to code under `cfg(target_os = "macos")`, including the Seatbelt sandbox profile. Linux CI and `cargo mutants` never compile that code.
- Walk the confirmation prompt and the recovery override in a real terminal after any change to rendering in `aegis-tui`.
- Send a command through a live Claude Code session after any change to `aegis hook` or `aegis install-hooks`.
- Take a real git snapshot in a throwaway repository and roll it back with `aegis rollback` after any change to snapshot or rollback code.
