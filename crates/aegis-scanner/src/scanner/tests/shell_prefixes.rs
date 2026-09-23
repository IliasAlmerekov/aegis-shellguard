use super::*;

#[test]
fn trailing_subshell_redirect_preserves_program_risk() {
    assert_assessment_matches_pattern(
        "(git push --force origin main) 2>&1",
        RiskLevel::Warn,
        "GIT-003",
    );
}

#[test]
fn shell_grammar_prefixes_preserve_program_risk() {
    const COMMAND: &str = "git push --force origin main";
    let wrapped = [
        COMMAND.to_string(),
        format!("({COMMAND})"),
        format!("({COMMAND}) >/dev/null"),
        format!("({COMMAND}) &>/dev/null"),
        format!("({COMMAND}) < /dev/null"),
        format!("({COMMAND}) <<< x"),
        format!("({COMMAND}) # note"),
        format!("({COMMAND} 2>&1)"),
        format!("({COMMAND}) | cat -v"),
        format!("( ({COMMAND}) ) 2>&1"),
        format!("(({COMMAND})) 2>&1"),
        format!("(\n{COMMAND}\n) 2>&1"),
        format!("true && ({COMMAND}) 2>&1"),
        format!("x=$( ({COMMAND}) 2>&1 )"),
        format!("{{ {COMMAND}; }}"),
        format!("{{ {COMMAND}; }} 2>&1"),
        format!("{{\n{COMMAND}\n}} 2>&1"),
        format!("if {COMMAND}; then :; fi"),
        format!("if ({COMMAND}); then :; fi"),
        format!("if true; then {COMMAND}; fi"),
        format!("if true; then :; else {COMMAND}; fi"),
        format!("if false; then :; elif {COMMAND}; then :; fi"),
        format!("while {COMMAND}; do :; done"),
        format!("until {COMMAND}; do :; done"),
        format!("for i in 1; do {COMMAND}; done"),
        format!("! {COMMAND}"),
        format!("! ({COMMAND})"),
        format!("time {COMMAND}"),
        format!("time ({COMMAND})"),
        format!("exec {COMMAND}"),
        format!("builtin {COMMAND}"),
        format!("coproc {COMMAND}"),
        format!("case x in a) {COMMAND};; esac"),
        format!("case x in\ta) {COMMAND};; esac"),
        format!("f() {{ {COMMAND}; }}; f"),
        format!("function f {{ {COMMAND}; }}"),
        format!("function\tf {{ {COMMAND}; }}"),
    ];

    for command in wrapped {
        assert_assessment_matches_pattern(&command, RiskLevel::Warn, "GIT-003");
    }
}

#[test]
fn shell_prefixes_keep_block_level_detection() {
    for command in [
        "(rm -rf /)",
        "(rm -rf /) 2>&1",
        "(rm -rf /) | cat",
        "(rm -rf /) > out.txt",
        "((cd /; rm -rf /)) 2>&1",
    ] {
        assert_assessment_matches_pattern(command, RiskLevel::Block, "PS-006");
    }
}

#[test]
fn benign_shell_prefixes_remain_safe() {
    let scanner = scanner();
    for command in [
        "(ls) 2>&1",
        "{ echo hi; } 2>&1",
        "if true; then echo ok; fi",
        "exec ls",
    ] {
        assert_eq!(scanner.assess(command).risk, RiskLevel::Safe, "{command:?}");
    }
}

#[test]
fn wrapped_heredoc_preserves_interpreter_risk() {
    let command = "(python3 - <<'EOF'\np='x.txt'; open(p,'w').write('changed')\nEOF\n) 2>&1";
    assert_assessment_matches_pattern(command, RiskLevel::Warn, "EXEC-013");
    assert_assessment_matches_pattern(&format!("{command} | cat -v"), RiskLevel::Warn, "EXEC-013");
}
