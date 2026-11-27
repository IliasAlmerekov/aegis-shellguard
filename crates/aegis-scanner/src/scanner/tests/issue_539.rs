use std::ffi::OsStr;

use super::*;
use crate::scanner::git_stages::shell_is_zsh;
use crate::scanner::{AmbientEnv, CdUnresolved, GitStageReport, StageDir};

fn report_in(cmd: &str, ambient: AmbientEnv) -> GitStageReport {
    let scanner = scanner();
    let assessment = scanner.assess(cmd);
    scanner.git_stage_report_with(cmd, &assessment, ambient)
}

fn zsh(cmd: &str) -> GitStageReport {
    report_in(
        cmd,
        AmbientEnv {
            zsh: true,
            ..AmbientEnv::default()
        },
    )
}

fn bash(cmd: &str) -> GitStageReport {
    report_in(cmd, AmbientEnv::default())
}

fn dirs(report: &GitStageReport) -> Vec<StageDir> {
    report.redirected.iter().map(|s| s.dir.clone()).collect()
}

fn chain(steps: &[&str]) -> Vec<StageDir> {
    vec![StageDir::Chain(
        steps.iter().map(|s| (*s).to_string()).collect(),
    )]
}

fn unresolved(reason: CdUnresolved) -> Vec<StageDir> {
    vec![StageDir::Unresolved(reason)]
}

#[test]
fn ac1_zsh_runs_the_last_pipeline_stage_in_the_current_shell() {
    assert_eq!(
        dirs(&zsh("true | cd ../other && rm -rf y")),
        chain(&["../other"])
    );
}

#[test]
fn ac1_bash_without_lastpipe_keeps_the_pipeline_cd_in_a_subshell() {
    let report = bash("true | cd ../other && rm -rf y");

    assert!(report.needs_process_cwd);
    assert!(report.redirected.is_empty());
}

#[test]
fn ac2_zsh_directory_stack_forms_are_unsupported() {
    for cmd in ["cd +1 && rm -rf y", "cd -2 && rm -rf y"] {
        assert_eq!(
            dirs(&zsh(cmd)),
            unresolved(CdUnresolved::UnsupportedForm),
            "{cmd:?}"
        );
    }
}

#[test]
fn ac2_double_dash_keeps_a_plus_operand_literal_under_zsh() {
    assert_eq!(dirs(&zsh("cd -- +1 && rm -rf y")), chain(&["+1"]));
}

#[test]
fn ac2_bash_treats_a_plus_operand_as_a_relative_path() {
    assert_eq!(dirs(&bash("cd +1 && rm -rf y")), chain(&["+1"]));
}

#[test]
fn ac3_chdir_is_followed_like_cd() {
    assert_eq!(
        dirs(&bash("chdir ../other && rm -rf y")),
        chain(&["../other"])
    );
}

#[test]
fn ac3_wrapped_chdir_is_as_opaque_as_wrapped_cd() {
    for cmd in [
        "builtin chdir ../other && rm -rf y",
        "command chdir ../other && rm -rf y",
    ] {
        assert_eq!(
            dirs(&bash(cmd)),
            unresolved(CdUnresolved::Opaque),
            "{cmd:?}"
        );
    }
}

#[test]
fn ac3_eval_chdir_is_unresolved() {
    assert_eq!(
        dirs(&bash("eval chdir x && rm -rf y")),
        unresolved(CdUnresolved::Opaque)
    );
}

#[test]
fn ac3_scan_limit_with_chdir_reports_an_unresolved_target() {
    let nest =
        |word: &str| (0..10).fold(format!("{word} x"), |inner, _| format!("eval \"{inner}\""));
    let (deep, plain) = (nest("chdir"), nest("true"));

    assert_eq!(
        dirs(&bash(&format!("{deep} && rm -rf y"))),
        unresolved(CdUnresolved::Opaque)
    );
    assert!(bash(&format!("{plain} && rm -rf y")).redirected.is_empty());
}

#[test]
fn ac4_ln_or_mv_before_a_cd_makes_it_unresolved() {
    for cmd in [
        "ln -sfn /repo/B link && cd link && rm -rf y",
        "mv a link && cd link && rm -rf y",
        "sudo ln -sfn /repo/B /tmp/link && cd /tmp/link && rm -rf y",
        "(ln -sfn /repo/B /tmp/link) && cd /tmp/link && rm -rf y",
        "ln -sfn /repo/B /tmp/link | true && cd /tmp/link && rm -rf y",
    ] {
        assert_eq!(
            dirs(&bash(cmd)),
            unresolved(CdUnresolved::PathChanged),
            "{cmd:?}"
        );
    }
}

#[test]
fn ac4_the_taint_blocks_the_absolute_cd_recovery() {
    assert_eq!(
        dirs(&bash("cd nope-$X && ln -s a b && cd /abs && rm -rf y")),
        unresolved(CdUnresolved::PathChanged)
    );
}

#[test]
fn ac4_other_earlier_stages_do_not_taint_the_walk() {
    assert_eq!(
        dirs(&bash("mkdir -p build && cd build && rm -rf y")),
        chain(&["build"])
    );
}

#[test]
fn ac5_a_subshell_cdpath_does_not_leak() {
    assert_eq!(
        dirs(&bash("(export CDPATH=/tmp) && cd project && rm -rf y")),
        chain(&["project"])
    );
}

#[test]
fn ac5_a_subshell_physical_mode_does_not_leak() {
    assert_eq!(dirs(&bash("(set -P) && cd x && rm -rf y")), chain(&["x"]));
}

#[test]
fn ac5_a_zsh_pipeline_stage_scopes_cdpath_to_its_own_subshell() {
    assert_eq!(
        dirs(&zsh("export CDPATH=/x | cd p && rm -rf y")),
        chain(&["p"])
    );
}

#[test]
fn ac5_a_bash_pipeline_stage_scopes_lastpipe_to_its_own_subshell() {
    let report = bash("shopt -s lastpipe | cd p && rm -rf y");

    assert!(report.needs_process_cwd);
    assert!(report.redirected.is_empty());
}

#[test]
fn ac5_a_cdpath_set_in_the_current_shell_still_applies() {
    assert_eq!(
        dirs(&bash("export CDPATH=/tmp && cd project && rm -rf y")),
        unresolved(CdUnresolved::CdPathSet)
    );
}

#[test]
fn ac5_ambient_cdpath_still_applies_inside_a_subshell() {
    let ambient = AmbientEnv {
        cdpath_set: true,
        ..AmbientEnv::default()
    };

    assert_eq!(
        dirs(&report_in("(true) && cd project && rm -rf y", ambient)),
        unresolved(CdUnresolved::CdPathSet)
    );
}

#[test]
fn ac6_set_named_as_an_argument_is_not_physical_mode() {
    assert_eq!(
        dirs(&bash("printf '%s' set -P && cd x && rm -rf y")),
        chain(&["x"])
    );
}

#[test]
fn ac6_the_set_builtin_still_turns_physical_mode_on() {
    for cmd in [
        "set -P && cd x && rm -rf y",
        "builtin set -o physical && cd x && rm -rf y",
    ] {
        assert_eq!(
            dirs(&bash(cmd)),
            unresolved(CdUnresolved::PhysicalMode),
            "{cmd:?}"
        );
    }
}

#[test]
fn ac7_zsh_is_detected_by_file_name_with_real_shell_first() {
    let os = |value: &'static str| Some(OsStr::new(value));

    assert!(shell_is_zsh(None, os("/bin/zsh")));
    assert!(shell_is_zsh(os("/usr/local/bin/zsh"), os("/bin/bash")));
    assert!(shell_is_zsh(os(""), os("/bin/zsh")));
    assert!(!shell_is_zsh(os("/bin/bash"), os("/bin/zsh")));
    assert!(!shell_is_zsh(None, os("/bin/bash")));
    assert!(!shell_is_zsh(None, os("/usr/local/bin/aegis")));
    assert!(!shell_is_zsh(None, None));
    assert!(!shell_is_zsh(os(""), os("")));
}
