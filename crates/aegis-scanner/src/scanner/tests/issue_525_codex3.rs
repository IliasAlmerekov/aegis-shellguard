use super::*;
use crate::scanner::{AmbientEnv, CdUnresolved, DangerStageTarget, GitStageReport, StageDir};

// #525, third Codex review of PR #529: the walk must never name a directory
// Bash would not enter. Each case here is one way the walk used to be wrong
// under Bash, the shell Aegis launches.

fn report_in(cmd: &str, ambient: AmbientEnv) -> GitStageReport {
    let scanner = scanner();
    let assessment = scanner.assess(cmd);
    scanner.git_stage_report_with(cmd, &assessment, ambient)
}

fn report(cmd: &str) -> GitStageReport {
    report_in(cmd, AmbientEnv::default())
}

fn dirs(report: &GitStageReport) -> Vec<StageDir> {
    report.redirected.iter().map(|s| s.dir.clone()).collect()
}

fn chain(steps: &[&str]) -> StageDir {
    StageDir::Chain(steps.iter().map(|s| (*s).to_string()).collect())
}

fn unresolved(reason: CdUnresolved) -> StageDir {
    StageDir::Unresolved(reason)
}

#[test]
fn a_redirection_glued_to_a_cd_operand_is_syntax_not_path_text() {
    let report = report("cd ../other>cd.log && rm -rf x");

    assert_eq!(dirs(&report), vec![chain(&["../other"])]);
}

#[test]
fn a_quoted_redirection_character_stays_part_of_the_cd_operand() {
    let report = report("cd '../other>cd.log' && rm -rf x");

    assert_eq!(dirs(&report), vec![chain(&["../other>cd.log"])]);
}

#[test]
fn an_exported_cd_function_makes_the_cd_unresolved() {
    let ambient = AmbientEnv {
        cd_function: true,
        ..AmbientEnv::default()
    };
    let report = report_in("cd ../other && rm -rf x", ambient);

    assert_eq!(dirs(&report), vec![unresolved(CdUnresolved::Opaque)]);
}

#[test]
fn inherited_physical_mode_makes_a_cd_unresolved() {
    let ambient = AmbientEnv {
        physical: true,
        ..AmbientEnv::default()
    };
    let report = report_in("cd link && cd .. && rm -rf y", ambient);

    assert_eq!(dirs(&report), vec![unresolved(CdUnresolved::PhysicalMode)]);
}

#[test]
fn an_extglob_operand_is_dynamic() {
    for operand in ["@(repo)", "+(repo)", "!(repo)"] {
        let report = report(&format!("cd {operand} && rm -rf y"));

        assert_eq!(
            dirs(&report),
            vec![unresolved(CdUnresolved::DynamicPath)],
            "{operand}"
        );
    }
}

#[test]
fn a_quoted_extglob_looking_operand_stays_literal() {
    let report = report("cd '@(repo)' && rm -rf y");

    assert_eq!(dirs(&report), vec![chain(&["@(repo)"])]);
}

#[test]
fn a_cd_ending_a_pipeline_after_lastpipe_changes_the_shell_directory() {
    let report = report("shopt -s lastpipe && true | cd ../other && rm -rf y");

    assert_eq!(dirs(&report), vec![chain(&["../other"])]);
}

#[test]
fn a_cd_ending_a_pipeline_stays_in_a_subshell_without_lastpipe() {
    let report = report("true | cd ../other && rm -rf y");

    assert!(dirs(&report).is_empty(), "{:?}", report.redirected);
    assert!(report.needs_process_cwd);
}

#[test]
fn command_with_an_option_before_cd_hides_the_directory_change() {
    let report = report("command -p cd ../other && rm -rf y");

    assert_eq!(dirs(&report), vec![unresolved(CdUnresolved::Opaque)]);
}

#[test]
fn builtin_wrapped_cd_hides_the_directory_change() {
    let report = report("builtin cd ../other && rm -rf y");

    assert_eq!(dirs(&report), vec![unresolved(CdUnresolved::Opaque)]);
}

#[test]
fn sudo_login_mode_changes_the_directory_after_a_cd() {
    for launcher in ["sudo -i", "sudo --login", "sudo -iu root"] {
        let report = report(&format!("cd ../a && {launcher} rm -rf x"));

        assert_eq!(
            report.redirected.first().map(|s| s.target.clone()),
            Some(DangerStageTarget::LauncherChangesDirectory),
            "{launcher}"
        );
    }
}

/// The assessment of `cmd` plus a language-aware Danger match, as the
/// analysis stage adds one. The scanner cannot reproduce its id from tokens.
fn report_with_language_match(cmd: &str) -> GitStageReport {
    let scanner = scanner();
    let mut assessment = scanner.assess(cmd);
    let mut language = test_match_result("language-aware match", 0, 0);
    language.pattern = Arc::new(Pattern {
        id: "LANG-FS-DEL-R".into(),
        ..(*language.pattern).clone()
    });
    assessment.matched.push(language);
    scanner.git_stage_report_with(cmd, &assessment, AmbientEnv::default())
}

#[test]
fn a_language_only_danger_is_attributed_to_the_moved_stage() {
    let report = report_with_language_match(
        "cd -- ../other && python3 -c 'import shutil; shutil.rmtree(\"victim\")'",
    );

    assert_eq!(dirs(&report), vec![chain(&["../other"])]);
    assert!(!report.needs_process_cwd);
}

#[test]
fn a_language_only_danger_before_the_cd_keeps_the_process_cwd() {
    let report = report_with_language_match("python3 script.py && cd ../other && ls");

    assert!(report.needs_process_cwd);
}
