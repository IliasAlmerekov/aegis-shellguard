//! Regression coverage for a stage name reused before and after the `alias`
//! that redefines it (issue #437, F2): the alias-scan scope must track the
//! call site's own position in the command, not the first (or any single)
//! text match for the stage's raw words.

use super::*;

/// The expected route for either test below: the first `run ./evil.py`
/// predates the alias, so `run` is still an ordinary, unaliased program name
/// there — an unclaimed-but-not-flagged launcher operand, same as any other
/// unrecognized command with a relative-path argument. The second call comes
/// after `alias run=python3`, so it is exactly as opaque as calling `python3`
/// directly would have been.
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
    // `run ./evil.py` appears twice: once before `alias run=python3` and
    // once after. Scoping the alias scan to the first text match in the
    // command, as the router did before this fix, put both calls at that
    // first (pre-alias) position and missed the second, aliased call
    // entirely — this reproduces issue #437, F2.
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
