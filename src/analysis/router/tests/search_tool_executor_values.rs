use super::*;

#[test]
fn rg_pre_separate_token_path_routes() {
    assert_eq!(
        route("rg --pre ./pyx foo .", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

#[test]
fn rg_pre_equals_glued_path_routes() {
    assert_eq!(
        route("rg --pre=./pyx foo .", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

#[test]
fn ag_pager_separate_token_path_routes() {
    assert_eq!(
        route("ag --pager ./pyx foo", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

#[test]
fn ag_pager_equals_glued_path_routes() {
    assert_eq!(
        route("ag --pager=./pyx foo", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

#[test]
fn rg_pre_naming_an_interpreter_degrades() {
    assert_eq!(
        route("rg --pre 'python3 ./evil.py' foo .", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn ordinary_rg_and_ag_invocations_auto_approve() {
    for command in [
        "rg foo .",
        "rg -n TODO src/",
        "ag foo",
        "rg --pre-glob '*.gz' foo .",
    ] {
        assert_eq!(route(command, &[]), Vec::new(), "{command}");
    }
}

#[test]
fn rg_and_ag_option_words_past_bare_double_dash_auto_approve() {
    for command in ["rg -- --pre ./pyx foo .", "ag -- --pager ./pyx foo"] {
        assert_eq!(route(command, &[]), Vec::new(), "{command}");
    }
}

#[test]
fn rg_pre_before_bare_double_dash_still_routes() {
    assert_eq!(
        route("rg --pre ./pyx -- foo .", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}
