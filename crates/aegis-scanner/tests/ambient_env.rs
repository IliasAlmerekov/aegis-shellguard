//! `Scanner::git_stage_report` reads Aegis's own environment (#525, #471).
//!
//! This file is its own test binary and holds one test, so editing the process
//! environment cannot race another test.

use aegis_scanner::{CdUnresolved, DangerStageTarget, StageDir, scanner_for};

fn stages(cmd: &str) -> Vec<(StageDir, DangerStageTarget)> {
    let scanner = scanner_for(&[]).expect("built-in scanner");
    let assessment = scanner.assess(cmd);
    scanner
        .git_stage_report(cmd, &assessment)
        .redirected
        .into_iter()
        .map(|stage| (stage.dir, stage.target))
        .collect()
}

#[test]
fn git_stage_report_reads_cdpath_and_git_env_from_the_process() {
    let cd_rm = "cd x && rm -rf y";
    let cd_git = "cd x && git filter-branch --index-filter true";
    let chain = StageDir::Chain(vec!["x".to_string()]);
    // SAFETY: this binary runs this one test, so no other thread touches the
    // environment.
    unsafe {
        std::env::remove_var("CDPATH");
        std::env::remove_var("GIT_DIR");
        std::env::remove_var("GIT_WORK_TREE");
    }
    assert_eq!(
        stages(cd_rm),
        vec![(chain.clone(), DangerStageTarget::Plain)]
    );

    unsafe { std::env::set_var("CDPATH", "/somewhere") };
    assert_eq!(
        stages(cd_rm),
        vec![(
            StageDir::Unresolved(CdUnresolved::CdPathSet),
            DangerStageTarget::Plain
        )]
    );
    unsafe { std::env::remove_var("CDPATH") };

    assert_eq!(
        stages(cd_git),
        vec![(chain.clone(), DangerStageTarget::Plain)]
    );
    unsafe { std::env::set_var("GIT_DIR", "/elsewhere/.git") };
    assert_eq!(
        stages(cd_git),
        vec![(chain, DangerStageTarget::EnvironmentRedirect)]
    );
    unsafe { std::env::remove_var("GIT_DIR") };
}
