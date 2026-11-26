use super::*;

#[test]
fn script_dash_c_quoted_exec_prefixed_interpreter_command_is_routed() {
    assert_eq!(
        route(r#"script -c "exec python3 ./evil.py""#, &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn script_dash_c_quoted_env_assignment_prefixed_interpreter_command_is_routed() {
    assert_eq!(
        route(r#"script -c "FOO=bar python3 ./evil.py""#, &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn script_dash_c_quoted_nohup_prefixed_interpreter_command_is_routed() {
    assert_eq!(
        route(r#"script -c "nohup python3 ./evil.py""#, &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn script_dash_c_quoted_prefix_word_with_no_interpreter_is_not_routed() {
    assert_eq!(route(r#"ssh host "exec echo hello""#, &[]), Vec::new());
}
