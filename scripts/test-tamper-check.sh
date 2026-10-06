#!/usr/bin/env bash
# Flag changes that weaken tests instead of fixing the code under test.
#
# Scans every added and removed line between the diff base and the working
# tree (committed, staged, unstaged, and untracked files) and reports:
#
#   Rust (*.rs)        an added #[ignore, #[should_panic, #[cfg(any())],
#                      #[cfg(FALSE)], or a commented-out #[test]
#   Rust test code     an added todo!(, unimplemented!(, or bare `return;`
#   JS/TS test files   an added .skip( .only( xit( xdescribe( .todo(
#   Python test files  an added pytest.mark.skip, xfail, pytest.skip(,
#                      unittest.skip
#   Any test code      a file that loses more assertion lines than it gains
#
# Test code is a file under a tests/ directory, a file named like a test
# (*_test.*, *_tests.rs, *.test.*, *.spec.*, test_*.py), or a Rust file that
# contains #[cfg(test)] or #[test] at the base or in the working tree.
#
# Escape: when a human approves a change that trips the check (deleting an
# obsolete test, say), put this line in the body of the latest commit message:
#
#   tamper-check: allow <reason>
#
# The check then prints the reason and passes. Reviewers see the line in the
# commit and decide whether the reason holds.
#
# Usage: scripts/test-tamper-check.sh [--base <ref>]
#   The default base is `git merge-base origin/${DEFAULT_BRANCH:-main} HEAD`.
#
# Exit codes: 0 no violations (or allowed), 1 violations, 2 usage or git error.
set -euo pipefail

usage() {
    echo "usage: scripts/test-tamper-check.sh [--base <ref>]" >&2
    exit 2
}

base_ref=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --base)
            [ "$#" -ge 2 ] || usage
            base_ref="$2"
            shift 2
            ;;
        -h | --help) usage ;;
        *) usage ;;
    esac
done

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

if [ -n "$base_ref" ]; then
    base=$(git rev-parse --verify --quiet "$base_ref^{commit}") || {
        echo "tamper-check: cannot resolve base ref $base_ref" >&2
        exit 2
    }
else
    upstream="origin/${DEFAULT_BRANCH:-main}"
    base=$(git merge-base "$upstream" HEAD) || {
        echo "tamper-check: no merge-base between $upstream and HEAD; pass --base <ref>" >&2
        exit 2
    }
fi

allow_reason=$(git log -1 --format=%b HEAD |
    sed -n 's/^[[:space:]]*tamper-check: allow[[:space:]]\{1,\}\(.*[^[:space:]]\)[[:space:]]*$/\1/p' |
    head -n 1)
if [ -n "$allow_reason" ]; then
    echo "tamper-check: skipped, HEAD commit allows it: $allow_reason"
    exit 0
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# One unified diff with zero context: tracked changes from the base to the
# working tree, then every untracked file as an addition.
git diff -U0 -M --no-color --no-ext-diff "$base" >"$work/diff"
git ls-files --others --exclude-standard -z >"$work/untracked"
while IFS= read -r -d '' path; do
    # --no-index exits 1 when the files differ, which they always do here.
    git diff -U0 --no-color --no-ext-diff --no-index -- /dev/null "$path" >>"$work/diff" || true
done <"$work/untracked"

# Rust files that hold inline tests, judged at both ends of the diff so that a
# change deleting the last #[test] in a file still counts as test code.
inline_test_marker='#\[cfg\(test\)\]|#\[test\]'
: >"$work/inline"
{
    git diff --name-only -M "$base"
    tr '\0' '\n' <"$work/untracked"
} | sort -u | while IFS= read -r path; do
    case "$path" in
        *.rs) ;;
        *) continue ;;
    esac
    if { [ -f "$path" ] && grep -Eq "$inline_test_marker" "$path"; } ||
        git show "$base:$path" 2>/dev/null | grep -Eq "$inline_test_marker"; then
        printf '%s\n' "$path" >>"$work/inline"
    fi
done
# Renamed files report their old path in the diff header too.
git diff --name-status -M "$base" | awk -F'\t' '$1 ~ /^R/ { print $2 }' |
    while IFS= read -r old; do
        if git show "$base:$old" 2>/dev/null | grep -Eq "$inline_test_marker"; then
            printf '%s\n' "$old" >>"$work/inline"
        fi
    done

status=0
awk -v inline_file="$work/inline" '
function is_test_path(p) {
    return p ~ /(^|\/)tests\// || p ~ /_tests?\.[^\/]+$/ || p ~ /\.(test|spec)\.[^\/]+$/ ||
        p ~ /(^|\/)test_[^\/]*\.py$/
}
function lang(p) {
    if (p ~ /\.rs$/) return "rust"
    if (p ~ /\.(js|jsx|mjs|cjs|ts|tsx|mts|cts)$/) return "js"
    if (p ~ /\.py$/) return "py"
    return "other"
}
function report(line, reason) {
    printf "%s:%d: %s\n", path, line, reason
    violations++
}
function flush() {
    if (path != "" && test_code && removed_asserts > added_asserts) {
        report(first_removed_assert,
            sprintf("removes %d assertion line(s) and adds %d", removed_asserts, added_asserts))
    }
    removed_asserts = 0; added_asserts = 0; first_removed_assert = 0
}
function start(p) {
    flush()
    path = p
    kind = lang(p)
    test_code = is_test_path(p) || (p in inline)
}
function removed_line(text) {
    if (test_code && text ~ assertion) {
        removed_asserts++
        if (first_removed_assert == 0) first_removed_assert = old_line
    }
    old_line++
}
function added_line(text) {
    if (test_code && text ~ assertion) added_asserts++
    if (kind == "rust") {
        if (text ~ /#\[ignore/) report(new_line, "adds #[ignore]")
        if (text ~ /#\[should_panic/) report(new_line, "adds #[should_panic]")
        if (text ~ ("#\\[cfg\\(any\\(" sp "*\\)\\)\\]")) report(new_line, "adds #[cfg(any())], which compiles the item out")
        if (text ~ /#\[cfg\(FALSE\)\]/) report(new_line, "adds #[cfg(FALSE)], which compiles the item out")
        if (text ~ ("//" sp "*#\\[(tokio::)?test")) report(new_line, "comments out a #[test]")
        if (test_code) {
            if (text ~ /(^|[^[:alnum:]_])todo!\(/) report(new_line, "adds todo!() to test code")
            if (text ~ /(^|[^[:alnum:]_])unimplemented!\(/) report(new_line, "adds unimplemented!() to test code")
            if (text ~ ("^" sp "*return;" sp "*$")) report(new_line, "adds a bare return; to test code")
        }
    } else if (kind == "js" && test_code) {
        if (text ~ /\.skip\(/) report(new_line, "adds .skip(")
        if (text ~ /\.only\(/) report(new_line, "adds .only(")
        if (text ~ /(^|[^[:alnum:]_.])xit\(/) report(new_line, "adds xit(")
        if (text ~ /(^|[^[:alnum:]_.])xdescribe\(/) report(new_line, "adds xdescribe(")
        if (text ~ /\.todo\(/) report(new_line, "adds .todo(")
    } else if (kind == "py" && test_code) {
        if (text ~ /pytest\.mark\.skip/) report(new_line, "adds pytest.mark.skip")
        if (text ~ /xfail/) report(new_line, "adds xfail")
        if (text ~ /pytest\.skip\(/) report(new_line, "adds pytest.skip(")
        if (text ~ /unittest\.skip/) report(new_line, "adds unittest.skip")
    }
    new_line++
}
BEGIN {
    while ((getline l < inline_file) > 0) inline[l] = 1
    sp = "[[:space:]]"
    # Rust assert macros, expect(, and a bare assert: Python `assert x` and
    # `assert(x)`, JS `assert(x)` and `assert.strictEqual(...)`.
    assertion = "(assert(_eq|_ne)?!|debug_assert[[:alnum:]_]*!|prop_assert[[:alnum:]_]*!|expect\\(|(^|[^[:alnum:]_.])assert(" sp "|\\.|\\())"
}
# Inside a hunk the header counts say how many lines follow, so a removed
# line that starts with "--" is not mistaken for a file header.
in_hunk && /^-/ { removed_line(substr($0, 2)); if (--old_left <= 0 && new_left <= 0) in_hunk = 0; next }
in_hunk && /^\+/ { added_line(substr($0, 2)); if (--new_left <= 0 && old_left <= 0) in_hunk = 0; next }
/^\\/ { next }
/^diff --git / { flush(); path = ""; old_path = ""; in_hunk = 0; next }
/^--- / {
    old_path = substr($0, 5)
    sub(/^a\//, "", old_path)
    next
}
/^\+\+\+ / {
    p = substr($0, 5)
    sub(/^b\//, "", p)
    if (p == "/dev/null") p = old_path
    start(p)
    # A rename keeps its old path in the inline set; carry that over.
    if (!test_code && (old_path in inline)) test_code = 1
    next
}
/^@@ / {
    split($0, f, " ")
    o = f[2]; n = f[3]
    sub(/^-/, "", o); sub(/^\+/, "", n)
    old_left = (o ~ /,/) ? substr(o, index(o, ",") + 1) + 0 : 1
    new_left = (n ~ /,/) ? substr(n, index(n, ",") + 1) + 0 : 1
    sub(/,.*/, "", o); sub(/,.*/, "", n)
    old_line = o + 0; new_line = n + 0
    in_hunk = (path != "") && (old_left > 0 || new_left > 0)
    next
}
END {
    flush()
    exit(violations > 0 ? 1 : 0)
}
' "$work/diff" || status=$?

if [ "$status" -eq 1 ]; then
    echo "tamper-check: test code was weakened. Fix the code instead, or record a" >&2
    echo "human-approved reason as 'tamper-check: allow <reason>' in the HEAD commit body." >&2
fi
exit "$status"
