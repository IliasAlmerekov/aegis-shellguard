# Aegis developer tasks
# Note: recipes use bare `cargo` for human use. In agent workflows run
# `rtk cargo …` directly (e.g. `rtk cargo run --bin aegis_schema`).

default:
    @just --list

# Generate JSON schema for aegis.toml editor autocompletion
write-config-schema:
    cargo run --bin aegis_schema

# Run all tests
test:
    cargo test

# Run rustfmt and clippy exactly as CI does
lint:
    scripts/lint.sh

# Build release binary
build:
    cargo build --release
