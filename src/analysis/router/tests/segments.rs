//! New router tests for issue #384 (route every segment of a compound
//! command). Split from `router::tests` to stay under the repo's 800-line
//! file-size budget; `use super::*` reaches through to the same `router` test
//! imports (`RoutedTarget`, `SourceLanguage`, `route`, ...).

use super::*;

// ── Every top-level segment is routed (issue #384) ──────────────────────────
//
// Routing used to look only at the command's first effective token, so a
// script hidden behind a leading no-op (`true; python3 evil.py`) or any other
// list operator routed nothing at all — the language-aware analysis stage
// never even started, and the command auto-approved.

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
    // The producer's stdout is irrelevant here: `python3 ./evil.py` names its
    // own script file, so it routes exactly as it would outside a pipeline —
    // the second documented gap in #384 (pipeline routing only recognized a
    // *bare* last stage reading piped stdin).
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
    // `cd x` (no `--`) is not the literal, tracked shape, and it appears
    // after a `;` rather than leading — both are new to #384: routing used to
    // track a `cd` only when it led the whole command.
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
    // The `&&`-chained script rebases onto the literal cwd; crossing the `;`
    // after it means the next script can no longer trust that cwd.
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
    // A `cd` running as a pipeline stage runs in a subshell — its effect on
    // the cwd never reaches its sibling stage (ADR-022 §6).
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
    // Only the narrow, exactly-two-stage `printf '%s' <literal> | <interp>`
    // shape is statically recoverable; a bare interpreter at a later stage of
    // a longer chain still reads piped stdin, so it is honest Dynamic
    // degradation rather than silently yielding no target at all.
    let targets = route("true | true | python3", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}

// ── A heredoc marker no longer sends the whole command down a first-segment-
// only legacy path (issue #384) ───────────────────────────────────────────
//
// Before this fix, any `<<WORD` in the command routed the whole thing
// through a single-segment fallback, so a real command hiding behind an
// inert heredoc-consuming no-op (`:`, `cat`) was never reached.

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
