#!/bin/sh
# Run rustfmt and clippy exactly as CI does.
#
# This script is the only place these two commands are written (#276). CI, the
# pre-push hook and `just lint` all call it, and `tests/lint_gate_ci.rs` fails
# if any of them grows its own copy again.
#
# Usage: scripts/lint.sh [fmt|clippy]
#   fmt     check formatting
#   clippy  lint every target and feature, warnings are errors
#   (none)  both, fmt first
#
# Both run on the toolchain pinned by RUST_TOOLCHAIN in .github/versions.env,
# not on the local default. rustfmt output and clippy lints change between
# releases, so a newer local toolchain would fail on code CI accepts, or pass
# code CI rejects.
set -eu

cd "$(dirname "$0")/.."

toolchain=$(sed -n 's/^RUST_TOOLCHAIN=//p' .github/versions.env)
if [ -z "$toolchain" ]; then
    echo "lint: RUST_TOOLCHAIN is missing from .github/versions.env" >&2
    exit 1
fi

# Stop rather than install the toolchain or fall back to another one: an
# install downloads hundreds of megabytes, and a fallback is the drift this
# script exists to remove.
require_component() {
    if ! cargo "+$toolchain" "$1" --version >/dev/null 2>&1; then
        echo "lint: $1 for Rust $toolchain is not installed. Install it with:" >&2
        echo "    rustup toolchain install $toolchain --component clippy,rustfmt" >&2
        exit 1
    fi
}

run_fmt() {
    require_component fmt
    echo "lint: cargo +$toolchain fmt --check --all"
    cargo "+$toolchain" fmt --check --all
}

run_clippy() {
    require_component clippy
    echo "lint: cargo +$toolchain clippy (all targets, all features)"
    cargo "+$toolchain" clippy --workspace --all-targets --all-features --locked -- -D warnings
}

case "${1:-all}" in
    fmt) run_fmt ;;
    clippy) run_clippy ;;
    all)
        run_fmt
        run_clippy
        ;;
    *)
        echo "usage: scripts/lint.sh [fmt|clippy]" >&2
        exit 2
        ;;
esac
