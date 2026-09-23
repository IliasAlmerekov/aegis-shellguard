//! cwd-tracking regression tests for issue #384 findings S2 (a second
//! literal `cd` must join onto the tracked cwd, not replace it) and S3 (`cd`
//! detection must see through launcher/assignment prefixes and group/
//! subshell wrappers, degrading rather than trusting a real cwd it cannot
//! resolve). Split from `router::tests` to stay under the file-size budget.

use super::*;

// ── S2: a second literal `cd` joins onto the tracked cwd ────────────────────

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
    // The first `cd` is not the tracked literal shape (an expansion, no
    // `--`), so the cwd is already unknown; a second, literal-looking `cd`
    // must not resurrect trust in it (ADR-022 §6: degraded never recovers).
    let targets = route(r#"cd "$D" && cd -- sub && python3 ./evil.py"#, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
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

// ── S3: `cd` detection sees through launcher/assignment prefixes and
// group/subshell wrappers, degrading rather than trusting a stale cwd ──────

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
    // A group's `cd` persists to the caller's cwd in a real shell, so
    // treating it as untracked (`Unset`) would resolve `./sub/evil.py`
    // against the wrong directory; the router cannot resolve the group's
    // effect, so it must degrade instead.
    let targets = route("{ cd -- d1; }; python3 ./sub/evil.py", &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::Dynamic {
            language: SourceLanguage::Python,
            reason: DegradationReason::DynamicSource,
        }]
    );
}
