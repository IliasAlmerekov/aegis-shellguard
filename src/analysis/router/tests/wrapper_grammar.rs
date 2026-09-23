//! Router unit tests pinning that a `case` pattern's own `)` (or `|`) inside
//! a wrapper is shell grammar, not the wrapper's own closing punctuation.
//! Split from `router::tests` to stay under the file-size budget.

use super::*;

fn evil_py_script_file() -> RoutedTarget {
    RoutedTarget::ScriptFile {
        language: SourceLanguage::Python,
        path: PathBuf::from("./evil.py"),
    }
}

// ── a case arm's `)` does not close an enclosing subshell early ────────────

#[test]
fn a_case_arm_inside_a_subshell_routes_the_arm_body() {
    assert_eq!(
        route("( case x in x) python3 ./evil.py;; esac )", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_case_arm_inside_a_subshell_with_no_inner_spacing_routes_the_arm_body() {
    assert_eq!(
        route("(case x in x)python3 ./evil.py;; esac)", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_case_arm_inside_a_chained_subshell_routes_the_arm_body() {
    assert_eq!(
        route("true && ( case x in x) python3 ./evil.py;; esac )", &[]),
        vec![evil_py_script_file()]
    );
}

#[test]
fn a_case_arm_inside_a_command_substitution_routes_the_arm_body() {
    assert_eq!(
        route("echo $(case x in x) python3 ./evil.py;; esac)", &[]),
        vec![evil_py_script_file()]
    );
}

// ── a case pattern's `|` does not split the stage as a pipe ────────────────

#[test]
fn a_case_pattern_alternation_does_not_split_the_arm_body_as_a_pipe() {
    assert_eq!(
        route("case x in x|y) python3 ./evil.py;; esac", &[]),
        vec![evil_py_script_file()]
    );
}

// ── benign variants stay auto-approved ──────────────────────────────────────

#[test]
fn a_benign_case_arm_inside_a_subshell_stays_unrouted() {
    assert_eq!(route("( case x in x) echo ok;; esac )", &[]), Vec::new());
}
