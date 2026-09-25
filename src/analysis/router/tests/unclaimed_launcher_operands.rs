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

// ── GHSA-xj54: an earlier segment that can change HOME, or that runs
// something opaque enough that it might, degrades a later `~/rest` operand
// rather than trusting the caller-supplied home ────────────────────────────

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

// ── Rule A rewrite: HOME trust ends at ANY earlier variable-writing stage,
// not only one that names HOME specifically (GHSA-xj54) ────────────────────

#[test]
fn any_earlier_variable_writing_stage_degrades_a_later_tilde_operand() {
    for command in [
        "HOME=/tmp/e 2>/dev/null; setsid ~/pyx",
        "HOME=/tmp/e >/dev/null; setsid ~/pyx",
        "export HOME+=/x; setsid ~/pyx",
        "declare HOME[0]=/tmp/e; setsid ~/pyx",
        "printf -v HOME /tmp/e; setsid ~/pyx",
        // Glued form: `-vHOME` is `-v HOME` with no space, the same way
        // `getopts` reads a glued short option (round-3 review finding 4).
        "printf -vHOME /tmp/e; setsid ~/pyx",
        "mapfile -t HOME < f; setsid ~/pyx",
        "readarray HOME < f; setsid ~/pyx",
        "getopts a HOME; setsid ~/pyx",
        "declare -n r=HOME; r=/tmp/e; setsid ~/pyx",
        "let HOME=5; setsid ~/pyx",
        "for HOME in /tmp/e; do :; done; setsid ~/pyx",
        "select HOME in /tmp/e; do break; done; setsid ~/pyx",
        r#"a=HO; b=ME; printf -v "$a$b" /tmp/e; setsid ~/pyx"#,
        // Accepted false positive: PATH is not HOME, but routing cannot
        // statically rule out an indirect effect on HOME once *any* earlier
        // stage writes a variable.
        "export PATH=/x; setsid ~/pyx",
    ] {
        assert_eq!(
            route_with_home(command, &[], Some(Path::new("/home/u"))),
            vec![unresolved_dynamic()],
            "{command}"
        );
    }
}

// ── Regression: a `for`/`select` header's own list is a launcher operand
// candidate too, not only a `HomeState` write (round-3 review finding 2,
// GHSA-xj54) ─────────────────────────────────────────────────────────────

#[test]
fn a_for_loop_header_naming_home_itself_still_degrades() {
    // The loop variable is `HOME` itself: the operand-walk's own
    // HOME-assignment guard (decision D1) reads the bare `HOME` token in
    // the list header the same way it would in `unset HOME`, ahead of ever
    // reaching `/tmp/e`.
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
    // `x` never gets a static value of its own outside the loop, so the
    // body's `"$x"` stays unclaimed — but the loop's own list is exactly as
    // much a candidate as `setsid ./pyx`'s operand is, since bash assigns
    // `x=./pyx` and the body goes on to run it.
    assert_eq!(
        route(r#"for x in ./pyx; do "$x"; done"#, &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
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
    // A narrowed scan that trusts a body it *thinks* only prints the
    // variable would miss every one of these: `$_` reads bash's own
    // last-argument variable (the value `"$x"` just left behind),
    // `printf -v y %s "$x"` copies the value into a second variable `"$y"`
    // that then runs, and `man -P "$x" ls` hands the value to a program
    // whose own `-P` value runs rather than reads. The loop header's own
    // list routes the same way regardless of what the body does with the
    // variable, so all three still route (GHSA-xj54).
    for command in [
        r#"for x in ./pyx; do echo "$x"; "$_"; done"#,
        r#"for x in ./pyx; do printf -v y %s "$x"; "$y"; done"#,
        r#"for x in ./pyx; do man -P "$x" ls; done"#,
    ] {
        assert_eq!(
            route(command, &[]),
            vec![RoutedTarget::LauncherOperand {
                path: PathBuf::from("./pyx"),
            }],
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
    // Accepted false positive (recorded in ADR-022): the body only prints
    // the variable, but the header's own list routes whatever the body
    // does, since no static rule proves the loop variable never runs
    // later in the same shell session (after `done`, in a trap, from a
    // function defined earlier). Here the glob in the list (`dir/*`) also
    // fails the literal-path check on its own, so this shape degrades
    // twice over, not just from the unconditional loop rule.
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
        "kill $PID",
        // Known gap: a slash-less dynamic launcher operand cannot be
        // distinguished from data without changing ordinary shell commands.
        "setsid $SCRIPT",
        // Same gap: an operand that is one whole command substitution has
        // no slash of its own, however path-like the substitution's output.
        r#"setsid "$(echo ./pyx)""#,
    ] {
        assert_eq!(route(command, &[]), Vec::new(), "{command}");
    }
}

// ── Regression: an executor value's own path-like candidate used to return
// immediately and hide the rest of the stage, including the launcher's own
// operand (round-3 review finding 1, GHSA-xj54) ────────────────────────────

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
    // `LocalCommand`'s value names a known interpreter, so the whole stage
    // degrades — the first `-o` used to return its own `./nc.sh` candidate
    // immediately and never reach the second `-o` at all.
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

// ── Regression: a pure-assignment stage naming an executor environment
// variable ignored a path-like value entirely (round-3 review finding 7,
// GHSA-xj54, Rule B) ─────────────────────────────────────────────────────

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
    // The generic operand walk used to see the untouched `-o` value token a
    // second time once this scan stopped returning immediately, reading its
    // whole, unparsed text (`"ProxyCommand ./evil.py %h"`, spaces and all)
    // as a second, bogus candidate alongside the correctly extracted
    // `./evil.py`.
    assert_eq!(
        route("ssh -o 'ProxyCommand ./evil.py %h' host", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./evil.py"),
        }]
    );
}
