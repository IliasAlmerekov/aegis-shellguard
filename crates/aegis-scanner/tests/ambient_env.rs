//! `Scanner::git_stage_report` reads Aegis's own environment (#525, #471).

#[path = "../../../tests/support/isolated_test.rs"]
mod isolated_test;

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
    const CASE: &str = "AEGIS_SCANNER_ENV_CASE";
    if let Ok(case) = std::env::var(CASE) {
        let _completion = isolated_test::Completion;
        let cd_rm = "cd x && rm -rf y";
        let cd_git = "cd x && git filter-branch --index-filter true";
        let chain = StageDir::Chain(vec!["x".to_string()]);
        match case.as_str() {
            "baseline" => {
                assert_eq!(
                    stages(cd_rm),
                    vec![(chain.clone(), DangerStageTarget::Plain)]
                );
                assert_eq!(stages(cd_git), vec![(chain, DangerStageTarget::Plain)]);
            }
            "cdpath" => assert_eq!(
                stages(cd_rm),
                vec![(
                    StageDir::Unresolved(CdUnresolved::CdPathSet),
                    DangerStageTarget::Plain
                )]
            ),
            "git" => assert_eq!(
                stages(cd_git),
                vec![(chain, DangerStageTarget::EnvironmentRedirect)]
            ),
            "shellopts" => assert_eq!(
                stages("cd x && cd .. && rm -rf y"),
                vec![(
                    StageDir::Unresolved(CdUnresolved::PhysicalMode),
                    DangerStageTarget::Plain
                )]
            ),
            "function" => assert_eq!(
                stages(cd_rm),
                vec![(
                    StageDir::Unresolved(CdUnresolved::Opaque),
                    DangerStageTarget::Plain
                )]
            ),
            "bashopts" => assert_eq!(
                stages("true | cd x && rm -rf y"),
                vec![(chain, DangerStageTarget::Plain)]
            ),
            _ => panic!("unknown case"),
        }
    } else {
        for (case, variable) in [
            ("baseline", None),
            ("cdpath", Some(("CDPATH", "/somewhere"))),
            ("git", Some(("GIT_DIR", "/elsewhere/.git"))),
            ("shellopts", Some(("SHELLOPTS", "braceexpand:physical"))),
            ("function", Some(("BASH_FUNC_cd%%", "() { :; }"))),
            ("bashopts", Some(("BASHOPTS", "cmdhist:lastpipe"))),
        ] {
            let mut child = isolated_test::command();
            child.env(CASE, case);
            for key in [
                "CDPATH",
                "GIT_DIR",
                "GIT_WORK_TREE",
                "SHELLOPTS",
                "BASH_FUNC_cd%%",
                "BASHOPTS",
            ] {
                child.env_remove(key);
            }
            if let Some((key, value)) = variable {
                child.env(key, value);
            }
            isolated_test::run(&mut child);
        }
    }
}
