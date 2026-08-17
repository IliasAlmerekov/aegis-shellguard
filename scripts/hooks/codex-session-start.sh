#!/usr/bin/env bash
# aegis-hook-version: 2
# Codex SessionStart hook — reports Aegis' effective enforcement state.
# Installed to: ~/.codex/hooks/aegis-session-start.sh

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

# SessionStart must emit exactly one JSON response. Do not source the mutable
# managed helper here: a malformed helper could write output and corrupt the
# agent protocol. The minimal inline state logic above is the deliberate
# protocol-safe fallback.

if ! aegis_enforcement_enabled; then
  cat <<'JSON'
{
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "Aegis is disabled: commands run in unguarded passthrough. Run \"aegis on\" to re-enable enforcement; \"aegis status\" shows the effective state."
  }
}
JSON
  exit 0
fi

if aegis_ci_active && aegis_disabled_locally; then
  cat <<'JSON'
{
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "Aegis is enforced: the local disabled Toggle is overridden by CI. IMPORTANT: All Bash tool commands must be routed through aegis. Run every shell command as: aegis --command '<original command>'. The PreToolUse hook transparently rewrites unwrapped commands to this form, so you may issue commands normally. This is a security requirement enforced by aegis. If Aegis denies or blocks a command, do not suggest bypassing the guardrail, do not recommend shell-escape forms such as '! <command>', and do not instruct the user to run the same risky command outside Aegis. You may explain the risk, suggest verification steps, and hand the decision to the human operator."
  }
}
JSON
  exit 0
fi

cat <<'JSON'
{
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "IMPORTANT: All Bash tool commands must be routed through aegis. Run every shell command as: aegis --command '<original command>'. The PreToolUse hook transparently rewrites unwrapped commands to this form, so you may issue commands normally. This is a security requirement enforced by aegis. If Aegis denies or blocks a command, do not suggest bypassing the guardrail, do not recommend shell-escape forms such as '! <command>', and do not instruct the user to run the same risky command outside Aegis. You may explain the risk, suggest verification steps, and hand the decision to the human operator."
  }
}
JSON
