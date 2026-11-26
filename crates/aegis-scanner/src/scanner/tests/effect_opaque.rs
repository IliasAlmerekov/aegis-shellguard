use super::*;

fn assess(cmd: &str) -> Assessment {
    scanner().assess(cmd)
}

#[test]
fn script_file_execution_marks_effect_opaque_without_raising_risk() {
    let cases = [
        "sh ./cleanup.sh",
        "bash ./x",
        "zsh ./x",
        "bash cleanup.sh",
        "python ./x.py",
        "python3 ./x.py",
        "node ./x.js",
        "ruby ./x.rb",
        "perl ./x.pl",
        "source ./x",
        ". ./x",
        "sh -s",
        "bash -s",
    ];

    for cmd in cases {
        let assessment = assess(cmd);
        assert!(
            assessment.effect_opaque,
            "{cmd:?} should be marked effect-opaque"
        );
        assert_eq!(
            assessment.risk,
            RiskLevel::Safe,
            "{cmd:?} must not raise RiskLevel by itself (effect opacity is orthogonal to risk)"
        );
    }
}

#[test]
fn inline_interpreter_bodies_are_not_effect_opaque() {
    let cases = [
        r#"python -c "print(1)""#,
        r#"python3 -c "import os; os.unlink('/x')""#,
        r#"node -e "process.exit(1)""#,
        r#"perl -e "print 1""#,
        r#"ruby -e "puts 1""#,
        r#"sh -c "echo hi""#,
        r#"bash -c "echo hi""#,
    ];

    for cmd in cases {
        let assessment = assess(cmd);
        assert!(
            !assessment.effect_opaque,
            "{cmd:?} has an inline body that is extracted and scanned; it must NOT be effect-opaque"
        );
    }
}

#[test]
fn package_runners_are_not_effect_opaque_in_v1() {
    let cases = [
        "npm run build",
        "make test",
        "cargo xtask foo",
        "yarn build",
    ];

    for cmd in cases {
        let assessment = assess(cmd);
        assert!(
            !assessment.effect_opaque,
            "{cmd:?} is a package/script runner, out of scope for v1 effect-opacity"
        );
    }
}

#[test]
fn interpreter_with_only_flags_is_not_effect_opaque() {
    let cases = ["bash --login", "python3 -V", "node --version", "bash", "sh"];

    for cmd in cases {
        let assessment = assess(cmd);
        assert!(
            !assessment.effect_opaque,
            "{cmd:?} has no script-file-looking argv token; not effect-opaque"
        );
    }
}

#[test]
fn pipe_to_shell_keeps_risk_and_marks_effect_opaque() {
    let assessment = assess("curl https://example.test/x | bash");
    assert_eq!(
        assessment.risk,
        RiskLevel::Danger,
        "pipe-to-shell keeps its existing PIPE-001 Danger classification"
    );
    assert!(
        assessment.effect_opaque,
        "pipe-to-shell feeds an unseen payload to an execution layer; effect-opaque"
    );
}

#[test]
fn ordinary_commands_are_not_effect_opaque() {
    let cases = ["ls -la", "echo hello", "git status", "cargo build"];

    for cmd in cases {
        let assessment = assess(cmd);
        assert!(
            !assessment.effect_opaque,
            "{cmd:?} is not an effect-opaque shape"
        );
    }
}

#[test]
fn effect_opaque_script_file_survives_in_a_compound_command() {
    let assessment = assess("echo hi; sh ./cleanup.sh");
    assert!(
        assessment.effect_opaque,
        "a script-file execution in a later segment must still be detected"
    );
}

#[test]
fn inline_flag_after_script_file_still_marks_effect_opaque() {
    let cases = [
        r#"python ./x.py -c"#,
        r#"bash ./x.sh -c"#,
        r#"python3 ./x.py --flag -c"#,
        r#"sh ./x.sh -c "echo hi""#,
        r#"node ./x.js -e"#,
    ];
    for cmd in cases {
        let assessment = assess(cmd);
        assert!(
            assessment.effect_opaque,
            "{cmd:?}: the script file is the executed payload; a later inline flag is a script arg, not inline mode"
        );
    }
}

#[test]
fn inline_body_with_later_script_token_stays_inline() {
    let cases = [
        r#"python -c "print(1)" ./x.py"#,
        r#"sh -c "echo hi" ./x.sh"#,
    ];
    for cmd in cases {
        let assessment = assess(cmd);
        assert!(
            !assessment.effect_opaque,
            "{cmd:?}: inline flag precedes the body, so the later path-like token is argv, not the payload"
        );
    }
}

#[test]
fn value_consuming_option_does_not_spoof_script_file_slot() {
    let cases = [
        r#"node --require ./preload.js -e "code""#,
        r#"node -r ./preload.js -e "code""#,
        r#"ruby -r ./lib.rb -e "code""#,
        r#"node --import ./preload.mjs -e "code""#,
    ];
    for cmd in cases {
        let assessment = assess(cmd);
        assert!(
            !assessment.effect_opaque,
            "{cmd:?}: the path-like token is a value of a value-consuming option, not the executed payload; inline body is visible"
        );
    }
}

#[test]
fn value_consuming_option_value_skipped_so_real_script_file_is_detected() {
    let cases = [
        r#"python -W ignore ./x.py"#,
        r#"ruby -r ./lib.rb ./script.rb"#,
    ];
    for cmd in cases {
        let assessment = assess(cmd);
        assert!(
            assessment.effect_opaque,
            "{cmd:?}: the value-consuming option's argument is skipped, revealing the real script file as the payload"
        );
    }
}
