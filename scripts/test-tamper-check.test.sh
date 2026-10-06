#!/usr/bin/env bash
# Tests for scripts/test-tamper-check.sh. Each case builds a throwaway git
# repository under mktemp -d, so the run needs git and nothing else.
#
# Usage: scripts/test-tamper-check.test.sh
set -euo pipefail

script="$(cd "$(dirname "$0")" && pwd)/test-tamper-check.sh"
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

passed=0
failed=0

# A repository with one commit on main that origin/main points at: a library
# with an inline test module and an integration test.
new_repo() {
    local dir="$scratch/$1"
    mkdir -p "$dir/src" "$dir/tests"
    git -C "$dir" init -q -b main
    git -C "$dir" config user.name "tamper-check test"
    git -C "$dir" config user.email "tamper-check@example.invalid"
    git -C "$dir" config commit.gpgsign false
    git -C "$dir" config core.autocrlf false
    cat >"$dir/src/lib.rs" <<'EOF'
pub fn double(x: u32) -> u32 {
    x * 2
}

#[cfg(test)]
mod tests {
    use super::double;

    #[test]
    fn double_of_two_is_four() {
        assert_eq!(double(2), 4);
    }
}
EOF
    cat >"$dir/tests/double.rs" <<'EOF'
#[test]
fn double_of_zero_is_zero() {
    assert_eq!(demo::double(0), 0);
}

#[test]
fn double_of_three_is_six() {
    assert_eq!(demo::double(3), 6);
}
EOF
    git -C "$dir" add -A
    git -C "$dir" commit -q -m "base"
    git -C "$dir" update-ref refs/remotes/origin/main HEAD
    printf '%s\n' "$dir"
}

# expect <case name> <repo> <wanted exit code> <wanted output substring>
expect() {
    local name="$1" dir="$2" want_code="$3" want_text="$4" output code=0
    output=$(cd "$dir" && "$script" 2>&1) || code=$?
    if [ "$code" -eq "$want_code" ] && [[ "$output" == *"$want_text"* ]]; then
        echo "ok   $name"
        passed=$((passed + 1))
    else
        echo "FAIL $name: exit $code (wanted $want_code), output:"
        printf '%s\n' "$output" | sed 's/^/    /'
        echo "    wanted output containing: $want_text"
        failed=$((failed + 1))
    fi
}

# Case: #[ignore] added to an inline test, left uncommitted.
repo=$(new_repo inline-ignore)
sed -i 's/^    #\[test\]$/    #[test]\n    #[ignore]/' "$repo/src/lib.rs"
expect "inline #[ignore] in the working tree" "$repo" 1 "src/lib.rs:10: adds #[ignore]"

# Case: #[should_panic] added to an integration test, committed.
repo=$(new_repo should-panic)
sed -i '1a #[should_panic]' "$repo/tests/double.rs"
git -C "$repo" commit -q -am "test: expect a panic"
expect "committed #[should_panic]" "$repo" 1 "tests/double.rs:2: adds #[should_panic]"

# Case: an assert_eq! line removed from an integration test.
repo=$(new_repo removed-assert)
sed -i '/double(3), 6/d' "$repo/tests/double.rs"
git -C "$repo" commit -q -am "test: drop an assertion"
expect "removed assert_eq!" "$repo" 1 "tests/double.rs:8: removes 1 assertion line(s) and adds 0"

# Case: a #[test] attribute commented out.
repo=$(new_repo commented-test)
sed -i '6s|^#\[test\]$|// #[test]|' "$repo/tests/double.rs"
expect "commented-out #[test]" "$repo" 1 "tests/double.rs:6: comments out a #[test]"

# Case: a new untracked test file that adds todo!().
repo=$(new_repo untracked-todo)
cat >"$repo/tests/triple.rs" <<'EOF'
#[test]
fn triple_is_pending() {
    todo!()
}
EOF
expect "untracked test file with todo!()" "$repo" 1 "tests/triple.rs:3: adds todo!() to test code"

# Case: an inline test module deleted along with its file's last #[test].
repo=$(new_repo deleted-module)
sed -i '/^#\[cfg(test)\]$/,$d' "$repo/src/lib.rs"
expect "deleted inline test module" "$repo" 1 "src/lib.rs:11: removes 1 assertion line(s) and adds 0"

# Case: a clean change, new code and a new test with its own assertion.
repo=$(new_repo clean)
cat >>"$repo/tests/double.rs" <<'EOF'

#[test]
fn double_of_five_is_ten() {
    assert_eq!(demo::double(5), 10);
}
EOF
sed -i 's/x \* 2/x + x/' "$repo/src/lib.rs"
git -C "$repo" commit -q -am "refactor: add instead of multiply"
expect "clean change passes" "$repo" 0 ""

# Case: a moved assertion keeps the balance even.
repo=$(new_repo moved-assert)
sed -i 's/assert_eq!(demo::double(0), 0);/let got = demo::double(0);\n    assert_eq!(got, 0);/' "$repo/tests/double.rs"
expect "rewritten assertion passes" "$repo" 0 ""

# Case: the escape line in the HEAD commit body lets a removal through.
repo=$(new_repo allow)
sed -i '/double(3), 6/d' "$repo/tests/double.rs"
git -C "$repo" commit -q -am "test: drop the duplicate case" \
    -m "tamper-check: allow double_of_three duplicates double_of_two, approved by the maintainer"
expect "allow escape in the commit body" "$repo" 0 \
    "skipped, HEAD commit allows it: double_of_three duplicates double_of_two, approved by the maintainer"

# Case: --base picks the diff base explicitly.
repo=$(new_repo explicit-base)
sed -i '/double(3), 6/d' "$repo/tests/double.rs"
git -C "$repo" commit -q -am "test: drop an assertion"
output=$(cd "$repo" && "$script" --base HEAD 2>&1) && code=0 || code=$?
if [ "$code" -eq 0 ]; then
    echo "ok   --base HEAD sees no change"
    passed=$((passed + 1))
else
    echo "FAIL --base HEAD sees no change: exit $code, output: $output"
    failed=$((failed + 1))
fi

echo "$passed passed, $failed failed"
[ "$failed" -eq 0 ]
