#!/usr/bin/env bash
# Probe how an Aegis binary decides a list of commands. See --help.
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: scripts/probe-decisions.sh <aegis-binary> <cases-file>
       scripts/probe-decisions.sh --help

Prints one tab-separated row per command in <cases-file>. The cases file holds
one command per line; blank lines and lines starting with # are skipped.

Columns:
  1  command
  2  decision                   auto_approve, prompt, block (--output json)
  3  risk                       safe, warn, danger, block (--output json)
  4  snapshot requested         true or false (--output json, snapshot_plan)
  5  analysis status            from the audit entry of a second run in deny
                                mode, "-" when the entry has none
  6  degradation reasons        comma-separated, "-" when none

The second run exists because --output json does not say why a safe command
prompts; only the audit entry does.

Isolation: the script builds its own git workspace and its own HOME under
$TMPDIR (or /tmp) and removes both on exit. Each run goes through env -i with
only PATH, that HOME, AEGIS_REAL_SHELL=<path of true>, AEGIS_CI=0
and, in the deny run, AEGIS_FORCE_NO_TTY=1 and AEGIS_AUDIT_PATH inside that
HOME. The git setup uses that HOME too, with system config off. GIT_DIR,
GIT_WORK_TREE, GIT_INDEX_FILE and the GIT_CONFIG_* overrides are unset. The
real HOME and ~/.aegis are never read or written.

Requires bash, git and jq.
EOF
}

case "${1:-}" in
    -h | --help)
        usage
        exit 0
        ;;
esac
if [ "$#" -ne 2 ]; then
    usage >&2
    exit 2
fi
command -v jq >/dev/null 2>&1 || {
    echo "probe-decisions: jq is required" >&2
    exit 1
}

bin=$1
cases=$2
[ -f "$cases" ] || {
    echo "probe-decisions: no cases file: $cases" >&2
    exit 2
}
[ -x "$bin" ] || {
    echo "probe-decisions: not executable: $bin" >&2
    exit 2
}
bin="$(cd "$(dirname "$bin")" && pwd)/$(basename "$bin")"

unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_CONFIG_GLOBAL GIT_CONFIG_PARAMETERS GIT_CONFIG_COUNT GIT_TEMPLATE_DIR
# Absolute path: AEGIS_REAL_SHELL must name a file, and macOS has no /bin/true.
true_bin=$(type -P true)

root=$(mktemp -d "${TMPDIR:-/tmp}/probe-decisions.XXXXXX")
trap 'rm -rf "$root"' EXIT
fake_home="$root/home"
ws="$root/ws"
audit="$fake_home/audit.jsonl"
mkdir -p "$fake_home" "$ws"
# The caller's git config and hooks must not touch the probe workspace.
export HOME="$fake_home" XDG_CONFIG_HOME="$fake_home/.config" GIT_CONFIG_NOSYSTEM=1
# A git repo with one commit, so a Danger command gets an applicable Snapshot.
echo notes >"$ws/notes.txt"
git -C "$ws" init -q
git -C "$ws" -c core.autocrlf=false add -A
git -C "$ws" -c user.name=probe -c user.email=probe@example.invalid \
    -c commit.gpgsign=false commit -q -m init

while IFS= read -r cmd || [ -n "$cmd" ]; do
    case "$cmd" in
        '' | '#'*) continue ;;
    esac

    json=$(cd "$ws" && env -i PATH="$PATH" HOME="$fake_home" \
        AEGIS_REAL_SHELL="$true_bin" AEGIS_CI=0 \
        "$bin" -c "$cmd" --output json </dev/null 2>/dev/null || true)
    fields=$(jq -r '[.decision, .risk, .snapshot_plan.requested] | map(tostring) | join("\t")' \
        <<<"$json" 2>/dev/null || true)
    [ -n "$fields" ] || fields=$(printf 'error\terror\terror')

    rm -f "$audit" "$audit.lock"
    (cd "$ws" && env -i PATH="$PATH" HOME="$fake_home" \
        AEGIS_REAL_SHELL="$true_bin" AEGIS_CI=0 AEGIS_FORCE_NO_TTY=1 \
        AEGIS_AUDIT_PATH="$audit" "$bin" -c "$cmd" </dev/null >/dev/null 2>&1 || true)
    analysis=$(printf -- '-\t-')
    if [ -s "$audit" ]; then
        analysis=$(tail -n 1 "$audit" | jq -r \
            '[(.analysis.status // "-"), ((.analysis.degradation_reasons // []) | if length == 0 then "-" else join(",") end)] | join("\t")' \
            2>/dev/null || printf -- '-\t-')
    fi

    printf '%s\t%s\t%s\n' "$cmd" "$fields" "$analysis"
done <"$cases"
