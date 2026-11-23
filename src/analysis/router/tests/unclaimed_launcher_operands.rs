//! Launcher operands that need home expansion or cannot be routed literally.

use super::*;

#[test]
fn dynamic_launcher_operands_degrade() {
    for command in [
        "setsid $HOME/pyx",
        "setsid ${HOME}/pyx",
        r#"setsid "$(pwd)/pyx""#,
        "setsid `pwd`/pyx",
    ] {
        assert_eq!(route(command, &[]), vec![unresolved_dynamic()], "{command}");
    }
}

#[test]
fn slash_inside_a_command_substitution_does_not_make_an_operand_path_like() {
    let command = "gh pr create --title t --body \"$(cat <<'EOF'
The router benchmark lives in benches/router_bench.rs.
EOF
)\"";

    assert_eq!(route(command, &[]), Vec::new());
}

#[test]
fn slash_outside_a_command_substitution_still_degrades() {
    for command in [
        r#"setsid "$(echo '(')/pyx""#,
        r#"setsid "$(echo ')')/pyx""#,
        "setsid `echo x`/pyx",
        r#"setsid "$(cat ./a)$(cat ./b)/pyx""#,
    ] {
        assert_eq!(route(command, &[]), vec![unresolved_dynamic()], "{command}");
    }
}

#[test]
fn quoted_or_escaped_substitution_syntax_is_literal_path_text() {
    for command in [
        "setsid '$(x/pyx)'",
        r"setsid \$\(x/pyx\)",
        r#"setsid "\$(x/pyx)""#,
        "setsid '`d/pyx`'",
        "setsid '$(x'/'pyx)'",
        r#"setsid "$(echo x/pyx)" '$(echo x/pyx)'"#,
    ] {
        assert_eq!(route(command, &[]), vec![unresolved_dynamic()], "{command}");
    }
}

#[test]
fn ansi_c_quoting_does_not_let_substitution_masking_hide_an_operand() {
    for command in [
        r#"strace -o $'\'$(echo x' $HOME/pyx ')'"#,
        r#"taskset -c 0 $'\'$(echo x' $HOME/pyx ')'"#,
        r#"strace -o $'\'`echo x' $HOME/pyx '`'"#,
    ] {
        assert_eq!(route(command, &[]), vec![unresolved_dynamic()], "{command}");
    }
}

#[test]
fn tilde_operand_with_extra_slashes_stays_under_home() {
    assert_eq!(
        route_with_home("setsid ~//pyx", &[], Some(Path::new("/home/u"))),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("/home/u/pyx"),
        }]
    );
}

#[test]
fn unterminated_command_substitution_in_an_operand_degrades() {
    assert_eq!(
        route(r#"setsid "$(echo x ./pyx""#, &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn glob_launcher_operands_degrade() {
    for command in [
        "setsid ./ev*",
        "setsid ./e?x",
        "setsid ./e[v]x",
        "setsid ./{evx,b}",
        "du -sh ./build/*",
    ] {
        assert_eq!(route(command, &[]), vec![unresolved_dynamic()], "{command}");
    }
    assert_eq!(
        route_with_home("setsid ~/p*", &[], Some(Path::new("/home/u"))),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn tilde_launcher_operand_without_a_home_degrades() {
    assert_eq!(route("setsid ~/pyx", &[]), vec![unresolved_dynamic()]);
}

#[test]
fn named_user_tilde_launcher_operand_degrades() {
    assert_eq!(
        route_with_home("setsid ~bob/pyx", &[], Some(Path::new("/home/u"))),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn tilde_launcher_operand_uses_the_caller_supplied_home() {
    assert_eq!(
        route_with_home("setsid ~/pyx", &[], Some(Path::new("/home/u"))),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("/home/u/pyx"),
        }]
    );
}

#[test]
fn tilde_launcher_operand_does_not_follow_a_later_cd() {
    assert_eq!(
        route_with_home("cd /tmp; setsid ~/pyx", &[], Some(Path::new("/home/u"))),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("/home/u/pyx"),
        }]
    );
}

// GHSA-xj54
#[test]
fn a_home_change_or_opaque_earlier_segment_degrades_a_later_tilde_operand() {
    for command in [
        "HOME=/tmp/e; setsid ~/pyx",
        "export HOME=/tmp/e; setsid ~/pyx",
        "export HOME=/tmp/e && setsid ~/pyx",
        "declare -x HOME=/tmp/e; setsid ~/pyx",
        "unset HOME; setsid ~/pyx",
        "read HOME; setsid ~/pyx",
        r#"eval "HO""ME=/tmp/e"; setsid ~/pyx"#,
        "source ./env.sh; setsid ~/pyx",
        ". ./env.sh; setsid ~/pyx",
    ] {
        assert_eq!(
            route_with_home(command, &[], Some(Path::new("/home/u"))),
            vec![unresolved_dynamic()],
            "{command}"
        );
    }
}

#[test]
fn any_earlier_variable_writing_stage_degrades_a_later_tilde_operand() {
    for command in [
        "HOME=/tmp/e 2>/dev/null; setsid ~/pyx",
        "HOME=/tmp/e >/dev/null; setsid ~/pyx",
        "export HOME+=/x; setsid ~/pyx",
        "declare HOME[0]=/tmp/e; setsid ~/pyx",
        "printf -v HOME /tmp/e; setsid ~/pyx",
        "printf -vHOME /tmp/e; setsid ~/pyx",
        "mapfile -t HOME < f; setsid ~/pyx",
        "readarray HOME < f; setsid ~/pyx",
        "getopts a HOME; setsid ~/pyx",
        "declare -n r=HOME; r=/tmp/e; setsid ~/pyx",
        "let HOME=5; setsid ~/pyx",
        "for HOME in /tmp/e; do :; done; setsid ~/pyx",
        "select HOME in /tmp/e; do break; done; setsid ~/pyx",
        r#"a=HO; b=ME; printf -v "$a$b" /tmp/e; setsid ~/pyx"#,
        // Accepted false positive (GHSA-xj54 Rule A): any variable write withholds HOME trust.
        "export PATH=/x; setsid ~/pyx",
    ] {
        assert_eq!(
            route_with_home(command, &[], Some(Path::new("/home/u"))),
            vec![unresolved_dynamic()],
            "{command}"
        );
    }
}

#[test]
fn a_for_loop_header_naming_home_itself_still_degrades() {
    assert_eq!(
        route_with_home(
            "for HOME in /tmp/e; do :; done",
            &[],
            Some(Path::new("/home/u"))
        ),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn a_for_loop_header_routes_its_own_path_like_list_entries() {
    assert_eq!(
        route_with_home(
            "for v in /tmp/e; do :; done",
            &[],
            Some(Path::new("/home/u"))
        ),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("/tmp/e"),
        }]
    );
}

#[test]
fn a_for_loop_variable_run_in_its_own_body_routes_the_loop_list() {
    assert_eq!(
        route(r#"for x in ./pyx; do "$x"; done"#, &[]),
        vec![
            RoutedTarget::LauncherOperand {
                path: PathBuf::from("./pyx"),
            },
            unresolved_dynamic(),
        ]
    );
}

#[test]
fn a_select_loop_header_routes_its_own_path_like_list_entries() {
    assert_eq!(
        route("select x in ./pyx; do break; done", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

#[test]
fn a_for_loop_header_routes_its_list_whatever_the_body_does_with_the_variable() {
    // GHSA-xj54: loop header lists route regardless of how the body uses the variable.
    for command in [
        r#"for x in ./pyx; do echo "$x"; "$_"; done"#,
        r#"for x in ./pyx; do printf -v y %s "$x"; "$y"; done"#,
        r#"for x in ./pyx; do man -P "$x" ls; done"#,
    ] {
        assert_eq!(
            route(command, &[]),
            vec![
                RoutedTarget::LauncherOperand {
                    path: PathBuf::from("./pyx"),
                },
                unresolved_dynamic()
            ],
            "{command}"
        );
    }
}

#[test]
fn an_unterminated_for_loop_header_still_routes_its_list() {
    assert_eq!(
        route(r#"for x in ./p*; do "$x""#, &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn a_list_only_for_loop_that_never_reads_its_variable_still_prompts() {
    // Accepted false positive (ADR-022): loop lists route unconditionally.
    assert_eq!(
        route(r#"for f in dir/*; do echo "$f"; done"#, &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn a_tilde_operand_with_no_home_change_ahead_of_it_still_resolves() {
    for command in [
        "setsid ~/pyx",
        "cd /tmp; setsid ~/pyx",
        "echo $HOME; setsid ~/pyx",
        "printf '%s' x; setsid ~/pyx",
        "setsid ~/pyx; HOME=/tmp/e",
    ] {
        assert_eq!(
            route_with_home(command, &[], Some(Path::new("/home/u"))),
            vec![RoutedTarget::LauncherOperand {
                path: PathBuf::from("/home/u/pyx"),
            }],
            "{command}"
        );
    }
}

#[test]
fn name_only_programs_and_urls_keep_their_existing_operand_behavior() {
    for command in [
        "cp ~/a ~/b",
        "ls src/*.rs",
        "wc -l src/*.rs",
        "rm -f ./build/*.o",
        "wget -O out http://x/y*",
        // Known gap: a slash-less dynamic launcher operand cannot be
        // distinguished from data without changing ordinary shell commands.
    ] {
        assert_eq!(route(command, &[]), Vec::new(), "{command}");
    }
    assert_eq!(route("kill $PID", &[]), vec![unresolved_dynamic()]);
    assert_eq!(route("setsid $SCRIPT", &[]), vec![unresolved_dynamic()]);
    assert_eq!(
        route(r#"setsid "$(echo ./pyx)""#, &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn dynamic_argv_backticks_and_live_substitution_beside_nowdoc_degrade() {
    assert_eq!(route("rm `printf x`", &[]), vec![unresolved_dynamic()]);
    let command =
        "gh pr create --body \"$(cat <<'EOF'\nplain text\nEOF\n)\" --title \"$(printf x)\"";
    assert!(route(command, &[]).contains(&unresolved_dynamic()));
}

#[test]
fn ifs_data_text_is_not_an_assignment() {
    assert_eq!(route("echo env IFS=,", &[]), Vec::new());
}

#[test]
fn dynamic_argv_keeps_independent_launcher_operand() {
    assert_eq!(
        route("setsid \"$x\" ./pyx", &[]),
        vec![
            unresolved_dynamic(),
            RoutedTarget::LauncherOperand {
                path: PathBuf::from("./pyx"),
            },
        ]
    );
}

#[test]
fn inert_dollar_word_does_not_degrade_without_live_expansion() {
    assert_eq!(route("rm '$x'", &[]), Vec::new());
    assert_eq!(route(r"rm \$x", &[]), Vec::new());
}

#[test]
fn an_env_prefix_executor_value_does_not_hide_the_stage_s_own_operand() {
    assert_eq!(
        route("EDITOR=/dev/null setsid ./pyx", &[]),
        vec![
            RoutedTarget::LauncherOperand {
                path: PathBuf::from("/dev/null"),
            },
            RoutedTarget::LauncherOperand {
                path: PathBuf::from("./pyx"),
            },
        ]
    );
}

#[test]
fn a_second_ssh_option_value_still_degrades_after_the_first_routes() {
    assert_eq!(
        route(
            "ssh -o ProxyCommand=./nc.sh -o LocalCommand='python3 ./evil.py' host",
            &[]
        ),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn every_distinct_ssh_option_launcher_operand_routes() {
    assert_eq!(
        route(
            "ssh -o ProxyCommand=./nc.sh -o LocalCommand=./run.sh host",
            &[]
        ),
        vec![
            RoutedTarget::LauncherOperand {
                path: PathBuf::from("./nc.sh"),
            },
            RoutedTarget::LauncherOperand {
                path: PathBuf::from("./run.sh"),
            },
        ]
    );
}

#[test]
fn a_pure_assignment_stage_routes_its_executor_variable_s_path_like_value() {
    assert_eq!(
        route("export PAGER=./pyx; man ls", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

#[test]
fn a_bare_assignment_stage_routes_its_executor_variable_s_path_like_value() {
    assert_eq!(
        route("EDITOR=./pyx; setsid true", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

#[test]
fn an_ordinary_command_is_unaffected_by_the_collect_and_route_fix() {
    for command in [
        "ssh -o ProxyCommand=none host",
        "export PATH=/x",
        "for f in a b c; do :; done",
        "man ls",
        "ls src/*.rs",
    ] {
        assert_eq!(route(command, &[]), Vec::new(), "{command}");
    }
    assert_eq!(
        route("setsid ./pyx", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

#[test]
fn a_space_form_ssh_option_value_does_not_also_route_as_its_own_literal_operand() {
    assert_eq!(
        route("ssh -o 'ProxyCommand ./evil.py %h' host", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./evil.py"),
        }]
    );
}
