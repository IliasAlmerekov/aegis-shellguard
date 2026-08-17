#!/usr/bin/env bash
# aegis-hook-version: 5
# Codex PreToolUse hook — transparently rewrites unwrapped Bash commands through
# aegis by delegating to the Rust `aegis hook` rewrite. No jq/python3 required.
# Installed to: ~/.codex/hooks/aegis-pre-tool-use.sh

set -u

aegis_truthy() {
  case "$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')" in
    1|true|yes) return 0 ;;
    *) return 1 ;;
  esac
}

aegis_falsy() {
  case "$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')" in
    0|false|no) return 0 ;;
    *) return 1 ;;
  esac
}

aegis_ci_active() {
  if [ -n "${AEGIS_CI:-}" ]; then
    aegis_falsy "${AEGIS_CI}" && return 1
    aegis_truthy "${AEGIS_CI}" && return 0
  fi

  for key in CI GITHUB_ACTIONS GITLAB_CI CIRCLECI BUILDKITE TRAVIS TF_BUILD; do
    value="$(printenv "$key" 2>/dev/null || true)"
    if [ -n "${value}" ] && aegis_truthy "${value}"; then
      return 0
    fi
  done

  [ -n "${JENKINS_URL:-}" ]
}

aegis_disabled_locally() {
  [ -f "${HOME}/.aegis/disabled" ]
}

aegis_enforcement_enabled() {
  if aegis_ci_active; then
    return 0
  fi

  if aegis_disabled_locally; then
    return 1
  fi

  return 0
}

AEGIS_TOGGLE_HELPER="${HOME}/.aegis/lib/toggle-state.sh"
if [ -r "${AEGIS_TOGGLE_HELPER}" ]; then
  . "${AEGIS_TOGGLE_HELPER}"
fi

if ! aegis_enforcement_enabled; then
  exit 0
fi

# Delegate JSON parsing and the allow+updatedInput rewrite to the Rust binary so
# behavior is identical to the Claude PreToolUse hook and does not depend on jq
# or python3 being installed. AEGIS_BIN is templated to an absolute, shell-quoted
# path at install time so the hook works even when the hook-exec PATH is minimal;
# an explicit AEGIS_BIN in the environment still wins (used by tests).
if [ -z "${AEGIS_BIN:-}" ]; then
  AEGIS_BIN=__AEGIS_BIN__
fi
if ! command -v "${AEGIS_BIN}" >/dev/null 2>&1; then
  # aegis binary unavailable — fail closed rather than let the command run
  # unscanned (ADR-007). Emit the same deny shape as `aegis hook` /
  # hook_deny_output: top-level reason + hookSpecificOutput.deny, exit 0.
  printf '%s\n' '{"reason":"aegis binary unavailable; refusing to run command unscanned","hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"aegis binary unavailable; refusing to run command unscanned"}}'
  exit 0
fi

# Run the binary (not exec) so the script survives the binary's death. Capture
# its stdout and record its exit status. Abnormal termination is defined as a
# non-zero exit status only: empty stdout with exit 0 stays a legitimate noop
# and is forwarded as silence. On abnormal termination the script emits its own
# deny response with a distinct reason and exits 0, symmetric to the
# binary-unavailable fail-closed path above. Double-printing is structurally
# impossible: a contained unwind exits 0 with the deny JSON, which the script
# merely forwards; an abnormal termination produces no stdout, and only then
# does the script speak (M4).
hook_output="$("${AEGIS_BIN}" hook)"
hook_status=$?
if [ "${hook_status}" -ne 0 ]; then
  printf '%s\n' '{"reason":"aegis hook terminated abnormally; refusing to run command unscanned","hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"aegis hook terminated abnormally; refusing to run command unscanned"}}'
  exit 0
fi
if [ -n "${hook_output}" ]; then
  printf '%s\n' "${hook_output}"
fi
exit 0
