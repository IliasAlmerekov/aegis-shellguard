//! Executor-carrying git `-c`/`--config-env` key families and environment
//! variables the unclaimed-interpreter net (issue #384/#430) must scan
//! beyond the original, narrower set: git config keys documented as running
//! a command rather than holding plain data, several of them with a
//! user-chosen middle segment (`credential.<url>.helper`, `gpg.<format>
//! .program`, `filter.<name>.clean`/`.smudge`/`.process`, `diff.<name>
//! .command`/`.textconv`, `merge.<name>.driver`); the matching environment
//! variables (`VISUAL`, `GIT_ASKPASS`, ...); and an executor assignment set
//! outside the stage's own leading-assignment prefix, either through the
//! `env` launcher or as a separate assignment stage (`export`/`declare -x`/
//! `typeset -x`/`readonly`, or a bare `NAME=value`) on the same line. Split
//! out of `unclaimed_interpreter_net.rs` to keep that file under this
//! project's line budget. `use super::*` reaches the same `router` test
//! imports (`RoutedTarget`, `SourceLanguage`, `route`, ...) its sibling
//! files use.

use super::*;

// ── A git config key documented as running a command is degraded, whether
// it is a fixed name or a family with a user-chosen middle segment
// (issue #384/#430) ─────────────────────────────────────────────────────

#[test]
fn git_dash_c_new_top_level_executor_keys_running_a_script_are_routed() {
    for command in [
        r#"git -c diff.external='python3 ./evil.py' diff"#,
        r#"git -c credential.helper='python3 ./evil.py' fetch"#,
        r#"git -c core.fsmonitor='python3 ./evil.py' status"#,
        r#"git -c sequence.editor='python3 ./evil.py' rebase -i HEAD~3"#,
        r#"git -c gpg.program='python3 ./evil.py' commit -S"#,
        r#"git -c core.askPass='python3 ./evil.py' fetch"#,
        r#"git -c core.gitProxy='python3 ./evil.py' fetch"#,
        r#"git -c uploadpack.packObjectsHook='python3 ./evil.py' upload-pack ."#,
        r#"git -c sendemail.sendmailcmd='python3 ./evil.py' send-email x"#,
    ] {
        assert_eq!(route(command, &[]), vec![unresolved_dynamic()], "{command}");
    }
}

#[test]
fn git_dash_c_executor_key_families_with_a_user_chosen_middle_segment_are_routed() {
    for command in [
        r#"git -c gpg.openpgp.program='python3 ./evil.py' commit -S"#,
        r#"git -c credential.https://example.com.helper='python3 ./evil.py' fetch"#,
        r#"git -c filter.lfs.clean='python3 ./evil.py' add x"#,
        r#"git -c filter.lfs.smudge='python3 ./evil.py' add x"#,
        r#"git -c filter.lfs.process='python3 ./evil.py' add x"#,
        r#"git -c diff.mydrv.command='python3 ./evil.py' diff"#,
        r#"git -c diff.mydrv.textconv='python3 ./evil.py' diff"#,
        r#"git -c merge.mydrv.driver='python3 ./evil.py' merge"#,
    ] {
        assert_eq!(route(command, &[]), vec![unresolved_dynamic()], "{command}");
    }
}

// ── An environment variable git or another executor reads for a command to
// run is degraded (issue #384/#430) ─────────────────────────────────────

#[test]
fn new_executor_env_vars_running_a_script_are_routed() {
    for command in [
        r#"VISUAL='python3 ./evil.py' git commit"#,
        r#"GIT_ASKPASS='python3 ./evil.py' git fetch"#,
        r#"SSH_ASKPASS='python3 ./evil.py' git fetch"#,
        r#"GIT_EXTERNAL_DIFF='python3 ./evil.py' git diff"#,
        r#"GIT_SEQUENCE_EDITOR='python3 ./evil.py' git rebase -i HEAD~3"#,
        r#"GIT_SSH='python3 ./evil.py' git fetch"#,
        r#"GIT_PROXY_COMMAND='python3 ./evil.py' git fetch"#,
        r#"BROWSER='python3 ./evil.py' git help status"#,
    ] {
        assert_eq!(route(command, &[]), vec![unresolved_dynamic()], "{command}");
    }
}

// ── An executor assignment past the stage's own leading-assignment prefix
// is still degraded: through the `env` launcher, or as its own assignment
// stage earlier on the same line (issue #384/#430) ──────────────────────

#[test]
fn env_launcher_wrapped_pager_assignment_running_a_script_is_routed() {
    assert_eq!(
        route(r#"env PAGER='python3 ./evil.py' man ls"#, &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn env_launcher_with_its_own_flag_before_the_assignment_is_routed() {
    assert_eq!(
        route(r#"env -i PAGER='python3 ./evil.py' man ls"#, &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn export_assignment_stage_naming_an_interpreter_is_routed() {
    assert_eq!(
        route(r#"export PAGER="python3 ./evil.py"; man ls"#, &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn bare_assignment_stage_naming_an_interpreter_is_routed() {
    assert_eq!(
        route(r#"PAGER="python3 ./evil.py"; man ls"#, &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn declare_dash_x_assignment_stage_naming_an_interpreter_is_routed() {
    assert_eq!(
        route(r#"declare -x PAGER="python3 ./evil.py"; man ls"#, &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn declare_with_combined_flags_assignment_stage_naming_an_interpreter_is_routed() {
    for command in [
        r#"declare -gx PAGER="python3 ./evil.py"; man ls"#,
        r#"declare -xg PAGER="python3 ./evil.py"; man ls"#,
        r#"declare -x -g PAGER="python3 ./evil.py"; man ls"#,
        r#"typeset -gx PAGER="python3 ./evil.py"; man ls"#,
    ] {
        assert_eq!(route(command, &[]), vec![unresolved_dynamic()], "{command}");
    }
}

#[test]
fn typeset_dash_x_assignment_stage_naming_an_interpreter_is_routed() {
    assert_eq!(
        route(r#"typeset -x PAGER="python3 ./evil.py"; man ls"#, &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn readonly_assignment_stage_naming_an_interpreter_is_routed() {
    assert_eq!(
        route(r#"readonly PAGER="python3 ./evil.py"; man ls"#, &[]),
        vec![unresolved_dynamic()]
    );
}

// ── A value or assignment naming no interpreter keeps today's auto-approve
// decision (issue #384/#430) ────────────────────────────────────────────

#[test]
fn git_dash_c_of_an_unrelated_key_family_shape_is_not_routed() {
    assert_eq!(
        route("git -c diff.algorithm=histogram diff", &[]),
        Vec::new()
    );
}

#[test]
fn export_assignment_stage_naming_no_interpreter_is_not_routed() {
    assert_eq!(route("export PAGER=less", &[]), Vec::new());
}

#[test]
fn env_launcher_wrapped_plain_assignment_is_not_routed() {
    assert_eq!(route("env LANG=C ls", &[]), Vec::new());
}
