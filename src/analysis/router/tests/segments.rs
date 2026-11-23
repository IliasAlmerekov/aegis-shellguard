//! Compound-command list-segment routing tests (#384).

use super::*;

#[test]
fn a_script_after_a_semicolon_is_routed() {
    assert_eq!(
        route("true; python3 script.py", &[]),
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("script.py"),
        }]
    );
}

#[test]
fn a_script_after_a_logical_and_is_routed() {
    assert_eq!(
        route("true && python3 script.py", &[]),
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("script.py"),
        }]
    );
}

#[test]
fn a_script_after_a_logical_or_is_routed() {
    assert_eq!(
        route("false || python3 script.py", &[]),
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("script.py"),
        }]
    );
}

#[test]
fn a_script_after_a_background_operator_is_routed() {
    assert_eq!(
        route("true & python3 script.py", &[]),
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("script.py"),
        }]
    );
}

#[test]
fn a_script_after_a_newline_is_routed() {
    assert_eq!(
        route("true\npython3 script.py", &[]),
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("script.py"),
        }]
    );
}

#[test]
fn a_pipeline_stage_with_its_own_script_argument_is_routed_like_a_standalone_command() {
    assert_eq!(
        route("true | python3 ./evil.py", &[]),
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("./evil.py"),
        }]
    );
}

#[test]
fn a_direct_exec_after_a_semicolon_is_routed() {
    assert_eq!(
        route("true; ./deploy.sh", &[]),
        vec![RoutedTarget::DirectExec {
            path: PathBuf::from("./deploy.sh"),
        }]
    );
}

#[test]
fn two_scripts_separated_by_a_semicolon_are_both_routed_in_order() {
    assert_eq!(
        route("python3 a.py; python3 b.py", &[]),
        vec![
            RoutedTarget::ScriptFile {
                language: SourceLanguage::Python,
                path: PathBuf::from("a.py"),
            },
            RoutedTarget::ScriptFile {
                language: SourceLanguage::Python,
                path: PathBuf::from("b.py"),
            },
        ]
    );
}

#[test]
fn an_inline_body_after_a_semicolon_is_routed() {
    assert_eq!(
        route(r#"true; python3 -c 'x'"#, &[]),
        vec![RoutedTarget::Inline {
            language: SourceLanguage::Python,
            source: "x".to_owned(),
        }]
    );
}

#[test]
fn a_mid_command_dynamic_cd_degrades_the_script_that_follows_it() {
    let targets = route("true; cd x && python3 a.py", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}

#[test]
fn a_literal_cd_rebases_only_the_directly_and_chained_segment() {
    let targets = route("cd -- /w && python3 a.py; python3 b.py", &[]);
    assert_eq!(
        targets,
        vec![
            RoutedTarget::ScriptFile {
                language: SourceLanguage::Python,
                path: PathBuf::from("/w/a.py"),
            },
            RoutedTarget::Dynamic {
                language: SourceLanguage::Python,
                reason: DegradationReason::DynamicSource,
            },
        ]
    );
}

#[test]
fn a_literal_cd_rebases_every_segment_in_an_unbroken_and_chain() {
    let targets = route("cd -- /w && python3 a.py && python3 b.py", &[]);
    assert_eq!(
        targets,
        vec![
            RoutedTarget::ScriptFile {
                language: SourceLanguage::Python,
                path: PathBuf::from("/w/a.py"),
            },
            RoutedTarget::ScriptFile {
                language: SourceLanguage::Python,
                path: PathBuf::from("/w/b.py"),
            },
        ]
    );
}

#[test]
fn a_cd_piped_into_the_next_stage_degrades_it() {
    let targets = route("cd -- /w | python3 a.py", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}

#[test]
fn a_three_stage_pipe_ending_in_a_bare_interpreter_degrades_dynamically() {
    let targets = route("true | true | python3", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}

#[test]
fn a_script_after_a_heredoc_consumed_by_a_no_op_is_routed() {
    let targets = route(": <<X\nhi\nX\ntrue; python3 ./evil.py", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("./evil.py"),
        }]
    );
}

#[test]
fn a_script_before_a_logical_and_with_a_trailing_heredoc_is_still_routed() {
    let targets = route("true && python3 ./evil.py <<X\nhi\nX", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("./evil.py"),
        }]
    );
}

#[test]
fn a_script_after_a_heredoc_consumed_by_cat_is_routed() {
    let targets = route("cat <<X\nhi\nX\npython3 ./evil.py", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("./evil.py"),
        }]
    );
}
