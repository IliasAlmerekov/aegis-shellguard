#!/usr/bin/env bash
# aegis-hook-version: 1
# Claude Code SessionStart hook — reports Aegis' effective enforcement state.
# Installed to: ~/.claude/hooks/aegis-session-start.sh

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

# WSL1 cannot create the user namespaces bubblewrap needs, so the Linux
# sandbox refuses commands there instead of running them unconfined
# (ADR-029 §3). Detected inline from /proc/version, mirroring the Rust
# detector in aegis-sandbox: an explicit wsl1 marker, or `microsoft` without
# the WSL2 `microsoft-standard` marker. AEGIS_SESSION_PROC_VERSION overrides
# the scanned content for tests; it only selects whether the constant sandbox
# note below is appended — the confinement decision itself is made by the
# aegis binary at command time, never here.
aegis_wsl1() {
  version="${AEGIS_SESSION_PROC_VERSION:-$(cat /proc/version 2>/dev/null || true)}"
  if [ -z "$version" ]; then
    return 1
  fi
  if printf '%s' "$version" | grep -qiE 'wsl1([^0-9]|$)'; then
    return 0
  fi
  if printf '%s' "$version" | grep -qi 'microsoft' &&
     ! printf '%s' "$version" | grep -qi 'microsoft-standard'; then
    return 0
  fi
  return 1
}

# SessionStart must emit exactly one JSON response. Do not source the mutable
# managed helper here: a malformed helper could write output and corrupt the
# agent protocol. The minimal inline state logic above is the deliberate
# protocol-safe fallback. The context arguments are JSON-escaped literals
# (the disabled text carries escaped quotes, so it stays single-quoted).
aegis_emit_session_json() {
  printf '{
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "%s%s"
  }
}
' "$1" "$2"
}

if ! aegis_enforcement_enabled; then
  aegis_emit_session_json 'Aegis is disabled: commands run in unguarded passthrough. Run \"aegis on\" to re-enable enforcement; \"aegis status\" shows the effective state.' ''
  exit 0
fi

# ADR-029 §3: warn at session start when the Linux sandbox cannot confine on
# this host (the WSL1 case). Appended only while enforcement is on — disabled
# passthrough makes no confinement claim, so there is nothing to warn about.
sandbox_note=''
if aegis_wsl1; then
  sandbox_note=" Aegis sandbox warning: WSL1 cannot create the user namespaces bubblewrap needs, so sandboxed commands are refused rather than run unconfined. Use WSL2 for sandboxed commands."
fi

if aegis_ci_active && aegis_disabled_locally; then
  aegis_emit_session_json "Aegis is enforced: the local disabled Toggle is overridden by CI. IMPORTANT: All Bash tool commands must be routed through aegis. Run every shell command as: aegis --command '<original command>'. The PreToolUse hook transparently rewrites unwrapped commands to this form, so you may issue commands normally. This is a security requirement enforced by aegis. If Aegis denies or blocks a command, do not suggest bypassing the guardrail, do not recommend shell-escape forms such as '! <command>', and do not instruct the user to run the same risky command outside Aegis. You may explain the risk, suggest verification steps, and hand the decision to the human operator." "$sandbox_note"
  exit 0
fi

aegis_emit_session_json "IMPORTANT: All Bash tool commands must be routed through aegis. Run every shell command as: aegis --command '<original command>'. The PreToolUse hook transparently rewrites unwrapped commands to this form, so you may issue commands normally. This is a security requirement enforced by aegis. If Aegis denies or blocks a command, do not suggest bypassing the guardrail, do not recommend shell-escape forms such as '! <command>', and do not instruct the user to run the same risky command outside Aegis. You may explain the risk, suggest verification steps, and hand the decision to the human operator." "$sandbox_note"