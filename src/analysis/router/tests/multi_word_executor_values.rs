//! A multi-word executor value routes its own leading path word, whichever
//! channel carried it — a glued short flag, a separate-token option, a
//! `--long=value` form, or a nested `NAME=value` (GHSA-xj54 follow-up).
//! `route_executor_value` already read a dispatched program's own value this
//! way (`man -P`, `git -c core.pager=`); the unclaimed net's own generic
//! per-token walk did not, so a program with no such dispatch (`tar`,
//! `rsync`) read the whole quoted value, spaces and all, as one literal
//! (and so nonexistent) path. `use super::*` reaches the same `router` test
//! imports its sibling files use.

use super::*;

// ── A multi-word value's leading path word routes as a launcher operand ────

#[test]
fn tar_glued_compress_program_multi_word_value_routes_its_leading_word() {
    assert_eq!(
        route("tar -I'./pyx -d' -cf out.tar dir", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

#[test]
fn tar_use_compress_program_long_option_multi_word_value_routes_its_leading_word() {
    assert_eq!(
        route("tar --use-compress-program='./pyx -d' -cf out.tar dir", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

#[test]
fn rsync_glued_remote_shell_multi_word_value_routes_its_leading_word() {
    assert_eq!(
        route("rsync -e'./pyx -p 22' a h:b", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

#[test]
fn rsync_remote_shell_separate_token_multi_word_value_routes_its_leading_word() {
    assert_eq!(
        route("rsync -e './pyx -p 22' a h:b", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

#[test]
fn man_pager_separate_token_multi_word_value_routes_its_leading_word() {
    assert_eq!(
        route("man -P './pyx -R' ls", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

// ── Already-covered by `route_executor_value`'s own leading-word extraction;
// pinned here alongside this file's other multi-word cases so the whole
// shape is covered in one place ─────────────────────────────────────────

#[test]
fn git_config_core_pager_multi_word_value_keeps_routing_its_leading_word() {
    assert_eq!(
        route("git -c core.pager='./pyx -R' log", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

// ── An interpreter-led multi-word value still degrades the stage, whatever
// channel carried it (unchanged behavior, pinned against regression) ───────

#[test]
fn interpreter_led_multi_word_values_still_degrade() {
    for command in [
        "tar -I'python3 ./evil.py' -cf out.tar dir",
        "rsync -e'python3 ./evil.py' a h:b",
        "man -P'python3 ./evil.py' ls",
    ] {
        assert_eq!(route(command, &[]), vec![unresolved_dynamic()], "{command}");
    }
}

// ── A multi-word value whose leading word names no interpreter and carries
// no path auto-approves, same as the single-word form ───────────────────────

#[test]
fn multi_word_values_with_no_path_leading_word_auto_approve() {
    for command in [
        "rsync -e 'ssh -p 22' a h:b",
        "tar -I 'gzip -9' -cf out.tar dir",
        "man -P 'less -R' ls",
    ] {
        assert_eq!(route(command, &[]), Vec::new(), "{command}");
    }
}

// ── A multi-word value led by a harmless path gets the exact decision the
// single-word form of that same path gets today ────────────────────────────

#[test]
fn a_multi_word_value_led_by_a_harmless_path_matches_its_single_word_decision() {
    assert_eq!(
        route("rsync -e'./okx -p 22' a h:b", &[]),
        route("rsync -e./okx a h:b", &[])
    );
}
