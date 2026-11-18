#!/bin/sh
# Run rustfmt, clippy and the text checks exactly as CI does.
#
# This script is the only place the rustfmt and clippy commands are written
# (#276). CI, the pre-push hook and `just lint` all call it, and
# `tests/lint_gate_ci.rs` fails if any of them grows its own copy again.
#
# Usage: scripts/lint.sh [fmt|clippy|text|all]
#   fmt     check formatting
#   clippy  lint every target and feature, warnings are errors
#   text    checks clippy cannot make, from grep and find only: no `.rs` file
#           under src/, crates/, tests/, fuzz/ or benches/ exceeds 800 lines
#   all     fmt, clippy, then text (also the default with no argument)
#
# LINT_TEXT_ROOT points the text checks at another directory than the
# checkout. The fixture tests use it. It changes nothing else.
#
# fmt and clippy run on the toolchain pinned by RUST_TOOLCHAIN in .github/versions.env,
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

# Every text check appends to `text_failed` instead of exiting, so one run
# reports all findings and a later check can join without touching the others.
text_failed=0

check_rust_file_length() {
    max=800
    scope=""
    for dir in src crates tests fuzz benches; do
        if [ -d "$dir" ]; then
            scope="$scope $dir"
        fi
    done
    [ -n "$scope" ] || return 0
    # `grep -c ''` counts a last line without a trailing newline, `wc -l` does not.
    # shellcheck disable=SC2086
    find $scope \( -name target -o -name .git -o -name .worktrees -o -name .cargo \
        -o -name .claude -o -name node_modules \) -prune \
        -o -type f -name '*.rs' -print |
        while IFS= read -r file; do
            lines=$(grep -c '' "$file" || true)
            if [ "$lines" -gt "$max" ]; then
                echo "$file: $lines lines, the limit is $max"
            fi
        done
}

run_text() {
    echo "lint: text checks"
    root=${LINT_TEXT_ROOT:-.}
    out=$(cd "$root" && check_rust_file_length)
    if [ -n "$out" ]; then
        echo "$out" >&2
        text_failed=1
    fi
    return "$text_failed"
}

case "${1:-all}" in
    fmt) run_fmt ;;
    clippy) run_clippy ;;
    text) run_text ;;
    all)
        run_fmt
        run_clippy
        run_text
        ;;
    *)
        echo "usage: scripts/lint.sh [fmt|clippy|text|all]" >&2
        exit 2
        ;;
esac
