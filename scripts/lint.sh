#!/bin/sh
# Run rustfmt, clippy and the text checks exactly as CI does.
#
# This script is the only place the rustfmt and clippy commands are written
# (#276). CI, the pre-push hook and `just lint` all call it, and
# `tests/lint_gate_ci.rs` fails if any of them grows its own copy again.
#
# Usage: scripts/lint.sh [fmt|clippy|comments|text|all]
#   fmt     check formatting
#   clippy  lint every target and feature, warnings are errors
#   comments  no comment in any tracked `.rs` file except `// SAFETY:`, found with
#           a Rust lexer (`cargo run -p xtask -- check-comments`)
#   text    checks clippy cannot make: no `.rs` file under src/, crates/,
#           tests/, fuzz/ or benches/ exceeds 800 lines, no `.rs` file under
#           src/, crates/ or tests/ carries a frozen tracker id in its name,
#           no comment in src/, crates/, tests/ or benches/ cites a review
#           round or finding id, every backticked path in AGENTS.md,
#           CONVENTION.md, CONTRIBUTING.md and CLAUDE.md exists in the tree,
#           no unguarded `Command::new("git")` spawns exist, and
#           scripts/test-tamper-check.test.sh passes
#   all     fmt, clippy, comments, then text (also the default with no argument)
#
# LINT_TEXT_ROOT points the text checks at another directory than the
# checkout. The fixture tests use it. It changes nothing else.
#
# fmt and clippy run on the toolchain pinned by RUST_TOOLCHAIN in
# .github/versions.env, not on the local default. rustfmt output and clippy
# lints change between releases, so a newer local toolchain would fail on code
# CI accepts, or pass code CI rejects.
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

# One shared exit status: run_text sets it, so a later check can join the others.
text_failed=0

# Populates `files` with `.rs` paths under the existing candidate dirs in "$@".
# Prints a diagnostic line when `find` fails to read a path.
find_rust_files() {
    files=""
    count=$#
    while [ "$count" -gt 0 ]; do
        dir=$1
        shift
        if [ -d "$dir" ]; then
            set -- "$@" "$dir"
        fi
        count=$((count - 1))
    done
    [ "$#" -gt 0 ] || return 0
    # Capture find first: in a pipeline its exit status would be lost.
    find_status=0
    files=$(find "$@" \( -name target -o -name .git -o -name .worktrees \
        -o -name .cargo -o -name .claude -o -name node_modules \) -prune \
        -o -type f -name '*.rs' -print) || find_status=$?
    if [ "$find_status" -ne 0 ]; then
        echo "find could not read every path under: $* (exit $find_status)"
    fi
}

# Prints one line per offender or read error, nothing when the tree is clean.
check_rust_file_length() {
    max=800
    find_rust_files src crates tests fuzz benches
    [ -n "$files" ] || return 0
    while IFS= read -r file; do
        # `grep -c ''` counts a last line without a trailing newline, `wc -l`
        # does not. It exits 1 on an empty file and 2 on a read error.
        grep_status=0
        lines=$(grep -c '' "$file") || grep_status=$?
        if [ "$grep_status" -gt 1 ]; then
            echo "$file: cannot read the file"
        elif [ "$lines" -gt "$max" ]; then
            echo "$file: $lines lines, the limit is $max"
        fi
    done <<EOF
$files
EOF
}

check_doc_paths() {
    for doc in AGENTS.md CONVENTION.md CONTRIBUTING.md CLAUDE.md; do
        [ -e "$doc" ] || continue
        grep_status=0
        tokens=$(grep -oE '`[^`]+`' "$doc") || grep_status=$?
        if [ "$grep_status" -gt 1 ]; then
            echo "$doc: cannot read the file"
            continue
        fi
        [ -n "$tokens" ] || continue
        while IFS= read -r raw; do
            token=${raw#\`}
            token=${token%\`}
            case "$token" in
                *[[:space:]]*) continue ;;
                */* | *.md | *.rs | *.toml | *.yml | *.sh) ;;
                *) continue ;;
            esac
            case "$token" in
                *'*'* | *'?'* | *'['*']'* | *'<'*'>'* | '~'* | *NNN*) continue ;;
                .md | .rs | .toml | .yml | .sh) continue ;;
            esac
            case "$token" in
                *[!/]*) ;;
                *) continue ;;
            esac
            case "$token" in
                # #616: project-level config file created at runtime, never committed.
                .aegis.toml) continue ;;
            esac
            case "$token" in
                /* | .. | ../* | */../* | */..)
                    echo "$doc: missing path \`$token\`"
                    continue
                    ;;
            esac
            if [ -e "$token" ]; then
                continue
            fi
            if git check-ignore -q -- "$token" 2>/dev/null; then
                continue
            fi
            echo "$doc: missing path \`$token\`"
        done <<EOF
$tokens
EOF
    done
}

# A git spawn outside these two files inherits a worktree hook's GIT_DIR (#291).
check_git_spawns() {
    set --
    for dir in src tests crates; do
        if [ -d "$dir" ]; then
            set -- "$@" "$dir"
        fi
    done
    [ "$#" -gt 0 ] || return 0
    grep_status=0
    files=$(grep -rlF --include='*.rs' --exclude-dir=target \
        'Command::new("git")' "$@") || grep_status=$?
    if [ "$grep_status" -gt 1 ]; then
        echo "grep could not read every path under: $* (exit $grep_status)"
    fi
    [ -n "$files" ] || return 0
    while IFS= read -r file; do
        case "$file" in
            src/test_git.rs | crates/aegis-snapshot/src/git/mod.rs) continue ;;
        esac
        echo "$file: spawns git without clearing GIT_ variables, see CONVENTION.md section 9"
    done <<EOF
$files
EOF
}

# A test or source file is named after its behaviour, not a frozen tracker id (#626).
check_tracker_id_filenames() {
    find_rust_files src crates tests
    [ -n "$files" ] || return 0
    while IFS= read -r file; do
        base=${file##*/}
        if printf '%s\n' "$base" | grep -qE '(^|_)[chm][0-9]+(_|\.rs$)'; then
            echo "$file: filename carries a tracker id, see CONVENTION.md section 9"
        fi
    done <<EOF
$files
EOF
}

# Comments do not cite review rounds or finding ids (#601, #629).
check_review_citations() {
    set --
    for dir in src crates tests benches; do
        if [ -d "$dir" ]; then
            set -- "$@" "$dir"
        fi
    done
    [ "$#" -gt 0 ] || return 0
    find_status=0
    files=$(find "$@" \( -name target -o -name .git -o -name .worktrees \
        -o -name .cargo -o -name .claude -o -name node_modules \) -prune \
        -o -type f \( -name '*.rs' -o -name '*.toml' \) -print) || find_status=$?
    if [ "$find_status" -ne 0 ]; then
        echo "find could not read every path under: $* (exit $find_status)"
    fi
    [ -n "$files" ] || return 0
    pat='([^A-Za-z0-9_]R[0-9]+-[0-9]+|#[0-9]+ review|finding [A-Z]?[0-9]+)'
    while IFS= read -r file; do
        case "$file" in
            *.rs) prefix='^[[:space:]]*//' ;;
            *.toml) prefix='^[[:space:]]*#' ;;
            *) continue ;;
        esac
        grep_status=0
        LC_ALL=C grep -qE "${prefix}.*${pat}" "$file" || grep_status=$?
        if [ "$grep_status" -gt 1 ]; then
            echo "$file: cannot read the file"
        elif [ "$grep_status" -eq 0 ]; then
            echo "$file: comment cites a review round or finding id, see CONVENTION.md section 4"
        fi
    done <<EOF
$files
EOF
}

run_comments() {
    echo "lint: cargo +$toolchain run -p xtask -- check-comments"
    cargo "+$toolchain" run --quiet --locked -p xtask -- check-comments
}

run_text() {
    echo "lint: text checks"
    root=${LINT_TEXT_ROOT:-.}
    out=$(cd "$root" && check_rust_file_length)
    if [ -n "$out" ]; then
        echo "$out" >&2
        text_failed=1
    fi
    out=$(cd "$root" && check_doc_paths)
    if [ -n "$out" ]; then
        echo "$out" >&2
        text_failed=1
    fi
    out=$(cd "$root" && check_git_spawns)
    if [ -n "$out" ]; then
        echo "$out" >&2
        text_failed=1
    fi
    out=$(cd "$root" && check_tracker_id_filenames)
    if [ -n "$out" ]; then
        echo "$out" >&2
        text_failed=1
    fi
    out=$(cd "$root" && check_review_citations)
    if [ -n "$out" ]; then
        echo "$out" >&2
        text_failed=1
    fi
    if [ -z "${LINT_TEXT_ROOT:-}" ] || [ -e "$root/scripts/test-tamper-check.test.sh" ]; then
        if ! out=$(cd "$root" && bash scripts/test-tamper-check.test.sh 2>&1); then
            echo "$out" >&2
            text_failed=1
        fi
    fi
    return "$text_failed"
}

case "${1:-all}" in
    fmt) run_fmt ;;
    clippy) run_clippy ;;
    comments) run_comments ;;
    text) run_text ;;
    all)
        run_fmt
        run_clippy
        run_comments
        run_text
        ;;
    *)
        echo "usage: scripts/lint.sh [fmt|clippy|comments|text|all]" >&2
        exit 2
        ;;
esac
