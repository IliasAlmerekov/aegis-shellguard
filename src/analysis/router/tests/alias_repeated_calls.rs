//! Alias-scan scoping when a stage name is reused before and after `alias` (#437).

use super::*;

fn expected_route() -> Vec<RoutedTarget> {
    vec![
        RoutedTarget::LauncherOperand {
            path: PathBuf::from("./evil.py"),
        },
        unresolved_dynamic(),
    ]
}

#[test]
fn a_call_repeated_before_and_after_its_alias_definition_is_routed() {
    assert_eq!(
        route("run ./evil.py; alias run=python3; run ./evil.py", &[]),
        expected_route()
    );
}

#[test]
fn a_call_repeated_before_and_after_its_alias_definition_on_separate_lines_is_routed() {
    assert_eq!(
        route("run ./evil.py\nalias run=python3\nrun ./evil.py", &[]),
        expected_route()
    );
}
