use super::*;

#[test]
fn two_chained_literal_cds_join_into_one_relative_base() {
    let targets = route("cd -- d1 && cd -- sub && python3 ./evil.py", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("d1/sub").join("./evil.py"),
        }]
    );
}

#[test]
fn a_dynamic_cd_followed_by_a_literal_cd_stays_degraded() {
    let targets = route(r#"cd "$D" && cd -- sub && python3 ./evil.py"#, &[]);
    assert_eq!(
        targets,
        vec![
            unresolved_dynamic(),
            RoutedTarget::Dynamic {
                language: SourceLanguage::Python,
                reason: DegradationReason::DynamicSource,
            },
        ]
    );
}

#[test]
fn a_second_absolute_cd_replaces_the_tracked_base_outright() {
    let targets = route("cd -- /a && cd -- /b && python3 evil.py", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("/b/evil.py"),
        }]
    );
}

#[test]
fn builtin_cd_is_recognized_and_degrades_a_later_relative_target() {
    let targets = route("builtin cd d1; python3 ./sub/evil.py", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}

#[test]
fn command_cd_is_recognized_and_degrades_a_later_relative_target() {
    let targets = route("command cd d1 && python3 ./sub/evil.py", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}

#[test]
fn an_env_prefixed_cd_is_recognized_and_degrades_a_later_relative_target() {
    let targets = route("X=1 cd d1 && python3 ./sub/evil.py", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}

#[test]
fn a_cd_inside_a_brace_group_degrades_a_later_relative_target() {
    let targets = route("{ cd -- d1; }; python3 ./sub/evil.py", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}

#[test]
fn a_cd_behind_a_bang_negation_degrades_a_later_relative_target() {
    let targets = route("! cd d1; python3 ./sub/evil.py", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}

#[test]
fn a_cd_in_an_if_condition_degrades_a_later_relative_target() {
    let targets = route("if cd d1; then :; fi; python3 ./sub/evil.py", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}

#[test]
fn a_cd_in_an_until_condition_degrades_a_later_relative_target() {
    let targets = route("until cd d1; do :; done; python3 ./sub/evil.py", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}

#[test]
fn a_cd_inside_a_for_loop_body_degrades_a_later_relative_target() {
    let targets = route("for d in d1; do cd $d; done; python3 ./sub/evil.py", &[]);
    assert_eq!(
        targets,
        vec![
            RoutedTarget::Unresolved {
                reason: DegradationReason::DynamicSource,
            },
            RoutedTarget::Dynamic {
                language: SourceLanguage::Python,
                reason: DegradationReason::DynamicSource,
            },
        ]
    );
}

#[test]
fn source_degrades_a_later_relative_target() {
    let targets = route("source ./x.sh; python3 ./sub/evil.py", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}

#[test]
fn eval_running_a_cd_degrades_a_later_relative_target() {
    let targets = route(r#"eval "cd /tmp/evil"; ./pyx"#, &[]);
    assert_eq!(targets, vec![unresolved_dynamic()]);
}

#[test]
fn eval_after_a_literal_cd_still_degrades_a_later_relative_target() {
    let targets = route("cd -- /a && eval x && ./pyx", &[]);
    assert_eq!(targets, vec![unresolved_dynamic()]);
}

#[test]
fn dot_source_stays_degraded_through_a_later_literal_cd() {
    let targets = route(". ./x.sh; cd -- sub && python3 ./evil.py", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}
