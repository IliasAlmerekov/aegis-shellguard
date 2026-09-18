#!/bin/sh
set -eu

BINDIR="${AEGIS_BINDIR:-/usr/local/bin}"
SHELL_RC_OVERRIDE="${AEGIS_SHELL_RC:-}"
NPM_PACKAGE="@iliasalmerekov/aegis"
PURGE_DATA="${AEGIS_UNINSTALL_PURGE_DATA:-}"

# Strip a single trailing slash from HOME so the string-built hook paths below
# (e.g. "${HOME}/.claude/hooks/aegis-pre-tool-use.sh") match the absolute path the
# Rust installer registers via std::path::absolute / Path::join, which never
# emits a doubled separator. Keep "/" intact so a root HOME still works.
HOME="${HOME%/}"
[ -n "${HOME}" ] || HOME="/"

DATA_DIR="${HOME}/.aegis"

BEGIN_MARKER="# >>> aegis shell setup >>>"
END_MARKER="# <<< aegis shell setup <<<"

cleanup() {
    if [ -n "${TMPDIR_AEGIS:-}" ] && [ -d "${TMPDIR_AEGIS}" ]; then
        rm -rf "${TMPDIR_AEGIS}"
    fi
}

trap cleanup EXIT INT TERM

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

need_cmd() {
    command -v "$1" >/dev/null 2>&1
}

target_path() {
    printf '%s/aegis\n' "${BINDIR}"
}

detect_real_shell() {
    aegis_path="$(target_path)"

    if [ -n "${AEGIS_REAL_SHELL:-}" ]; then
        real_shell="${AEGIS_REAL_SHELL}"
    elif [ -n "${SHELL:-}" ] && [ "${SHELL}" != "${aegis_path}" ]; then
        real_shell="${SHELL}"
    else
        fail "cannot determine which shell rc file to clean up; set AEGIS_REAL_SHELL or AEGIS_SHELL_RC and rerun"
    fi

    printf '%s\n' "${real_shell}"
}

resolve_rc_file() {
    real_shell="$1"

    if [ -n "${SHELL_RC_OVERRIDE}" ]; then
        printf '%s\n' "${SHELL_RC_OVERRIDE}"
        return
    fi

    shell_name="$(basename "${real_shell}")"

    case "${shell_name}" in
        bash)
            printf '%s/.bashrc\n' "${HOME}"
            ;;
        zsh)
            printf '%s/.zshrc\n' "${HOME}"
            ;;
        *)
            fail "automatic shell cleanup supports bash and zsh; set AEGIS_SHELL_RC for ${shell_name}"
            ;;
    esac
}

remove_managed_block() {
    input_path="$1"
    output_path="$2"

    if [ ! -f "${input_path}" ]; then
        : > "${output_path}"
        return
    fi

    awk -v begin="${BEGIN_MARKER}" -v end="${END_MARKER}" '
        $0 == begin { skip = 1; next }
        $0 == end { skip = 0; next }
        skip != 1 { print }
    ' "${input_path}" > "${output_path}"
}

remove_shell_setup() {
    rc_file="$1"
    tmp_rc="${TMPDIR_AEGIS}/rc.tmp"

    if [ ! -f "${rc_file}" ]; then
        return
    fi

    remove_managed_block "${rc_file}" "${tmp_rc}"
    mv "${tmp_rc}" "${rc_file}"
}

remove_binary() {
    install_target="$(target_path)"
    install_dir="$(dirname "${install_target}")"
    binary_status="absent"

    if [ ! -e "${install_target}" ]; then
        return
    fi

    if [ -w "${install_dir}" ]; then
        rm -f "${install_target}"
        binary_status="removed"
        return
    fi

    if need_cmd sudo; then
        sudo rm -f "${install_target}"
        binary_status="removed"
        return
    fi

    fail "cannot remove ${install_target}; rerun as root or install sudo"
}

# This script only ever removes the curl-installed binary at target_path(). It
# has no way to drive an npm or Homebrew uninstall itself, so after that
# removal it just checks whether `aegis` still resolves on PATH and, if so,
# tells the operator which command to run next — see #374.
detect_remaining_binary() {
    remaining_status="none"
    remaining_path=""

    if remaining_path="$(command -v aegis 2>/dev/null)"; then
        remaining_status="found"
    fi
}

describe_remaining_binary() {
    if [ "${remaining_status}" != "found" ]; then
        return
    fi

    if need_cmd npm && npm ls -g "${NPM_PACKAGE}" >/dev/null 2>&1; then
        printf 'warning: %s is still installed via npm (%s); this script only removed %s. Run `npm uninstall -g %s` to remove it.\n' \
            "${remaining_path}" "${NPM_PACKAGE}" "$(target_path)" "${NPM_PACKAGE}" >&2
        return
    fi

    printf 'warning: an aegis binary is still on PATH at %s; this script only removed %s. Remove it with whatever tool installed it (npm, Homebrew, a manual copy, etc.).\n' \
        "${remaining_path}" "$(target_path)" >&2
}

# ~/.aegis holds the audit log, snapshots, and the toggle flag — data an
# operator may want to keep around after uninstalling the binary. (The user
# config lives separately, at ~/.config/aegis/config.toml, and this script
# never touches it.) Only touch ~/.aegis when asked to, via
# AEGIS_UNINSTALL_PURGE_DATA=1 or --purge-data.
purge_data_dir() {
    [ "${PURGE_DATA}" = "1" ] || return 0
    [ -d "${DATA_DIR}" ] || return 0

    rm -rf "${DATA_DIR}"
}

remove_hook_payload() {
    hook_path="$1"
    hook_dir="$(dirname "${hook_path}")"

    if [ -e "${hook_path}" ] && ! [ -w "${hook_dir}" ]; then
        if need_cmd sudo; then
            sudo rm -f "${hook_path}"
            return
        fi

        fail "cannot remove ${hook_path}; rerun as root or install sudo"
    fi

    rm -f "${hook_path}"
}

prune_hook_registration() {
    json_file="$1"
    section="$2"
    command_path="$3"

    [ -f "${json_file}" ] || return 0

    jq --arg section "${section}" --arg cmd "${command_path}" '
        if .hooks[$section]? then
            .hooks[$section] = [
                .hooks[$section][]?
                | .hooks = [
                    .hooks[]?
                    | select(.type != "command" or .command != $cmd)
                  ]
                | select((.hooks | length) > 0)
            ]
        else
            .
        end
    ' "${json_file}" > "${TMPDIR_AEGIS}/hook-prune.tmp"

    mv "${TMPDIR_AEGIS}/hook-prune.tmp" "${json_file}"
}

parse_args() {
    for arg in "$@"; do
        case "${arg}" in
            --purge-data)
                PURGE_DATA="1"
                ;;
            *)
                fail "unknown argument: ${arg}"
                ;;
        esac
    done
}

main() {
    parse_args "$@"
    TMPDIR_AEGIS="$(mktemp -d)"
    data_dir_existed="false"
    if [ -d "${DATA_DIR}" ]; then
        data_dir_existed="true"
    fi

    if [ -f "${HOME}/.claude/settings.json" ] || [ -f "${HOME}/.codex/hooks.json" ]; then
        need_cmd jq || fail "jq is required to prune agent hook registrations"
    fi

    if [ -n "${SHELL_RC_OVERRIDE}" ]; then
        rc_file="$(resolve_rc_file "")"
    else
        real_shell="$(detect_real_shell)"
        rc_file="$(resolve_rc_file "${real_shell}")"
    fi
    remove_shell_setup "${rc_file}"
    remove_binary
    remove_hook_payload "${HOME}/.claude/hooks/aegis-rewrite.sh"
    remove_hook_payload "${HOME}/.claude/hooks/aegis-pre-tool-use.sh"
    remove_hook_payload "${HOME}/.claude/hooks/aegis-session-start.sh"
    remove_hook_payload "${HOME}/.codex/hooks/aegis-session-start.sh"
    remove_hook_payload "${HOME}/.codex/hooks/aegis-pre-tool-use.sh"
    remove_hook_payload "${HOME}/.aegis/lib/toggle-state.sh"
    prune_hook_registration "${HOME}/.claude/settings.json" "PreToolUse" "${HOME}/.claude/hooks/aegis-rewrite.sh"
    prune_hook_registration "${HOME}/.claude/settings.json" "PreToolUse" "${HOME}/.claude/hooks/aegis-pre-tool-use.sh"
    prune_hook_registration "${HOME}/.claude/settings.json" "PreToolUse" "aegis hook"
    prune_hook_registration "${HOME}/.claude/settings.json" "SessionStart" "${HOME}/.claude/hooks/aegis-session-start.sh"
    prune_hook_registration "${HOME}/.codex/hooks.json" "SessionStart" "${HOME}/.codex/hooks/aegis-session-start.sh"
    prune_hook_registration "${HOME}/.codex/hooks.json" "PreToolUse" "${HOME}/.codex/hooks/aegis-pre-tool-use.sh"
    purge_data_dir
    detect_remaining_binary

    printf 'Removed shell wrapper setup from %s\n' "${rc_file}"
    if [ "${binary_status}" = "removed" ]; then
        printf 'Removed %s\n' "$(target_path)"
    else
        printf 'No binary found at %s; nothing to remove there.\n' "$(target_path)"
    fi
    describe_remaining_binary

    if [ "${data_dir_existed}" = "true" ]; then
        if [ "${PURGE_DATA}" = "1" ]; then
            printf 'Removed data directory %s (audit log, snapshots, disabled flag)\n' "${DATA_DIR}"
        else
            printf 'Left data directory in place: %s (audit log, snapshots, disabled flag). Rerun with AEGIS_UNINSTALL_PURGE_DATA=1 or --purge-data to delete it.\n' "${DATA_DIR}"
        fi
    fi
}

main "$@"
