//! An executor-carrying option value or environment-variable assignment
//! naming a known interpreter degrades the stage even when its program sits
//! on the unclaimed-interpreter net's `NAME_ONLY_PROGRAMS` (issue #384/#430):
//! git `-c`/`--config-env`, man `-P`/`--pager=`, and leading `PAGER`-style
//! assignments. Split out of `unclaimed_interpreter_net.rs` to keep that file
//! under this project's line budget; `executor_config_keys.rs` covers the
//! wider key and variable families. `use super::*` reaches the same `router`
//! test imports its sibling files use.

use super::*;

fn unresolved_dynamic() -> RoutedTarget {
    RoutedTarget::Unresolved {
        reason: DegradationReason::DynamicSource,
    }
}

// ── An executor-carrying option value or environment-variable assignment is
// degraded even when the stage's own program sits on `NAME_ONLY_PROGRAMS`
// (issue #384/#430) ──────────────────────────────────────────────────────

#[test]
fn git_dash_c_core_pager_running_a_script_is_routed() {
    assert_eq!(
        route("git -c core.pager='python3 ./evil.py' log", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn git_dash_c_alias_shell_command_running_a_script_is_routed() {
    assert_eq!(
        route("git -c alias.x='!python3 ./evil.py' x", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn lessopen_env_prefix_running_a_script_is_routed() {
    assert_eq!(
        route("LESSOPEN='|python3 ./evil.py %s' less notes.txt", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn man_pager_flag_running_a_script_is_routed() {
    assert_eq!(
        route("man -P 'python3 ./evil.py' man", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn git_dash_c_core_editor_running_a_script_is_routed() {
    assert_eq!(
        route("git -c core.editor='python3 ./evil.py' commit", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn git_dash_c_core_ssh_command_running_a_script_is_routed() {
    assert_eq!(
        route("git -c core.sshCommand='python3 ./evil.py' fetch", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn git_config_env_running_a_script_is_routed() {
    assert_eq!(
        route("git --config-env core.pager=python3 log", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn git_pager_env_prefix_running_a_script_is_routed() {
    assert_eq!(
        route("GIT_PAGER='python3 ./evil.py' git log", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn pager_env_prefix_running_a_script_is_routed() {
    assert_eq!(
        route("PAGER='python3 ./evil.py' man ls", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn manpager_env_prefix_running_a_script_is_routed() {
    assert_eq!(
        route("MANPAGER='python3 ./evil.py' man ls", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn git_editor_env_prefix_running_a_script_is_routed() {
    assert_eq!(
        route("GIT_EDITOR='python3 ./evil.py' git commit", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn editor_env_prefix_running_a_script_is_routed() {
    assert_eq!(
        route("EDITOR='python3 ./evil.py' git commit", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn git_ssh_command_env_prefix_running_a_script_is_routed() {
    assert_eq!(
        route("GIT_SSH_COMMAND='python3 ./evil.py' git fetch", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn lessclose_env_prefix_running_a_script_is_routed() {
    assert_eq!(
        route("LESSCLOSE='python3 ./evil.py %s %s' less notes.txt", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn man_long_pager_flag_running_a_script_is_routed() {
    assert_eq!(
        route("man --pager='python3 ./evil.py' man", &[]),
        vec![unresolved_dynamic()]
    );
}

// ── A config value or env assignment naming no interpreter keeps today's
// auto-approve decision (issue #384/#430) ──────────────────────────────────

#[test]
fn git_dash_c_core_pager_of_a_plain_pager_is_not_routed() {
    assert_eq!(route("git -c core.pager=less log", &[]), Vec::new());
}

#[test]
fn pager_env_prefix_of_a_plain_pager_is_not_routed() {
    assert_eq!(route("PAGER=cat man ls", &[]), Vec::new());
}

#[test]
fn git_dash_c_of_an_unrelated_key_is_not_routed() {
    assert_eq!(route("git -c user.name=x commit", &[]), Vec::new());
}
