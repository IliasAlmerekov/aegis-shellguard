//! The fail-closed net for a stage no other routing path claims, once a
//! later token names a known interpreter (issue #384/#430, ADR-022 §6
//! amendment). `launcher_prefix_lengths` (`crates/aegis-parser/src/lib.rs`)
//! enumerates launcher words by name, so an unlisted wrapper (`setsid`,
//! `strace`, `ionice`, `taskset`, `find … -exec`, …) leaves its program
//! token unrecognized and the interpreter it wraps invisible to every other
//! routing path — this net catches what the closed list misses instead of
//! growing that list one word at a time. `use super::*` reaches the same
//! `router` test imports (`RoutedTarget`, `SourceLanguage`, `route`, ...)
//! `router::tests`'s other siblings use.

use super::*;

// ── An unenumerated wrapper hiding an interpreter is degraded ──────────────

#[test]
fn setsid_wrapped_interpreter_is_routed() {
    assert_eq!(
        route("setsid python3 ./evil.py", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn stdbuf_wrapped_interpreter_is_routed() {
    assert_eq!(
        route("stdbuf -oL python3 ./evil.py", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn strace_wrapped_interpreter_is_routed() {
    assert_eq!(
        route("strace python3 ./evil.py", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn ionice_wrapped_interpreter_is_routed() {
    assert_eq!(
        route("ionice python3 ./evil.py", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn taskset_wrapped_interpreter_is_routed() {
    assert_eq!(
        route("taskset 0x1 python3 ./evil.py", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn exec_dash_a_wrapped_interpreter_is_routed() {
    assert_eq!(
        route("exec -a fake python3 ./evil.py", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn command_dash_p_wrapped_interpreter_is_routed() {
    assert_eq!(
        route("command -p python3 ./evil.py", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn doas_wrapped_interpreter_is_routed() {
    assert_eq!(
        route("doas -u root python3 ./evil.py", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn time_dash_p_wrapped_interpreter_is_routed() {
    assert_eq!(
        route("time -p python3 ./evil.py", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn find_exec_interpreter_argument_is_routed() {
    assert_eq!(
        route(r"find . -name evil.py -exec python3 {} \;", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn wrapped_interpreter_after_a_semicolon_noop_is_routed() {
    assert_eq!(
        route("true; setsid python3 ./evil.py", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn wrapped_interpreter_inside_a_subshell_is_routed() {
    assert_eq!(
        route("(strace python3 ./evil.py)", &[]),
        vec![unresolved_dynamic()]
    );
}

// ── A program that only names a command as data stays unclaimed ────────────

#[test]
fn echo_naming_an_interpreter_is_not_routed() {
    assert_eq!(route("echo python3", &[]), Vec::new());
}

#[test]
fn which_naming_an_interpreter_is_not_routed() {
    assert_eq!(route("which python3", &[]), Vec::new());
}

#[test]
fn grep_naming_an_interpreter_is_not_routed() {
    assert_eq!(route("grep -r node src", &[]), Vec::new());
}

#[test]
fn apt_install_naming_an_interpreter_is_not_routed() {
    assert_eq!(route("apt install python3", &[]), Vec::new());
}

#[test]
fn git_log_grep_naming_an_interpreter_is_not_routed() {
    assert_eq!(route("git log --grep python3", &[]), Vec::new());
}

#[test]
fn man_naming_an_interpreter_is_not_routed() {
    assert_eq!(route("man bash", &[]), Vec::new());
}

#[test]
fn cat_reading_a_plain_file_is_not_routed() {
    assert_eq!(route("cat notes.txt", &[]), Vec::new());
}

#[test]
fn bare_interpreter_version_flag_is_not_routed() {
    assert_eq!(route("python3 --version", &[]), Vec::new());
}

#[test]
fn command_dash_v_lookup_is_not_routed() {
    assert_eq!(route("command -v python3", &[]), Vec::new());
}

#[test]
fn ls_piped_into_grep_is_not_routed() {
    assert_eq!(route("ls -la | grep foo; echo done", &[]), Vec::new());
}

#[test]
fn find_piped_into_xargs_grep_is_not_routed() {
    assert_eq!(
        route("find . -name '*.rs' | xargs grep foo", &[]),
        Vec::new()
    );
}

// ── An interpreter named inside a quoted multi-word token is routed ────────

#[test]
fn script_dash_c_quoted_interpreter_command_is_routed() {
    assert_eq!(
        route(r#"script -c "python3 ./evil.py""#, &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn script_dash_qc_quoted_interpreter_command_is_routed() {
    assert_eq!(
        route(r#"script -qc "python3 ./evil.py" /dev/null"#, &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn script_dash_c_quoted_exec_prefixed_interpreter_command_is_routed() {
    // review comment 4091038690: `exec` inside the quoted command string
    // still runs whatever follows it, so the interpreter behind it must be
    // as visible as it is with no prefix at all.
    assert_eq!(
        route(r#"script -c "exec python3 ./evil.py""#, &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn script_dash_c_quoted_env_assignment_prefixed_interpreter_command_is_routed() {
    // review comment 4091038690: a bare `NAME=value` assignment ahead of the
    // interpreter inside the quoted command string is exactly as opaque as
    // `exec` is — the assignment does not change which program runs.
    assert_eq!(
        route(r#"script -c "FOO=bar python3 ./evil.py""#, &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn script_dash_c_quoted_nohup_prefixed_interpreter_command_is_routed() {
    assert_eq!(
        route(r#"script -c "nohup python3 ./evil.py""#, &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn script_dash_c_quoted_prefix_word_with_no_interpreter_is_not_routed() {
    // "exec" ahead of a benign command carries no interpreter to catch, the
    // same way a plain quoted benign command does not.
    assert_eq!(route(r#"ssh host "exec echo hello""#, &[]), Vec::new());
}

#[test]
fn quoted_multiword_token_naming_a_benign_command_is_not_routed() {
    // "echo" is not a registered registry interpreter, so a quoted command
    // that merely starts with it carries no interpreter to catch, the same
    // way `echo python3` itself names one as data rather than running it.
    assert_eq!(route(r#"ssh host "echo hello""#, &[]), Vec::new());
}

// ── A program that never executes its operands stays unclaimed ─────────────

#[test]
fn mkdir_naming_an_interpreter_is_not_routed() {
    assert_eq!(route("mkdir python3", &[]), Vec::new());
}

#[test]
fn rm_dash_f_naming_an_interpreter_is_not_routed() {
    assert_eq!(route("rm -f node", &[]), Vec::new());
}

#[test]
fn touch_naming_an_interpreter_is_not_routed() {
    assert_eq!(route("touch python3", &[]), Vec::new());
}

#[test]
fn cp_naming_an_interpreter_is_not_routed() {
    assert_eq!(route("cp python3 /tmp/", &[]), Vec::new());
}

#[test]
fn mv_naming_an_interpreter_is_not_routed() {
    assert_eq!(route("mv node node.bak", &[]), Vec::new());
}

// ── A path-like operand after an unenumerated launcher is a launcher-operand candidate ──

#[test]
fn setsid_launcher_operand_is_routed() {
    assert_eq!(
        route("setsid ./pyx", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

#[tokio::test]
async fn setsid_launcher_operand_with_a_verified_shebang_prompts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pyx");
    std::fs::write(&path, "#!/usr/bin/env python3\nprint(1)\n").unwrap();

    let command = format!("setsid {}", path.display());
    let targets = route(&command, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::LauncherOperand { path: path.clone() }]
    );

    let results = resolve(targets, 1024).await;
    assert_eq!(
        results,
        vec![Ok(SourceTarget {
            language: SourceLanguage::Python,
            source: "#!/usr/bin/env python3\nprint(1)\n".to_owned(),
        })]
    );
}

#[tokio::test]
async fn setsid_launcher_operand_without_a_shebang_stays_auto_approved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.txt");
    std::fs::write(&path, "just notes, not a script\n").unwrap();

    let command = format!("setsid {}", path.display());
    let targets = route(&command, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::LauncherOperand { path: path.clone() }]
    );

    let results = resolve(targets, 1024).await;
    assert_eq!(results, Vec::new());
}

// ── A launcher operand that is missing or a directory stays speculative ────
//
// `resolve`/`resolve_for_analysis` read a launcher operand exactly like a
// user-typed direct-exec target, with one exception (issue #384/#430 round
// 5): a missing path or a literal directory is an everyday shape for an
// *ordinary* command's argument (`vim ./new.txt`, `du -sh ./srcdir`), not
// evidence of anything unsafe, so it drops silently instead of degrading —
// unlike a user-typed `./missing.sh`, which still degrades exactly as it did
// before this net existed.

#[tokio::test]
async fn vim_missing_file_operand_is_not_degraded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new.txt");

    let command = format!("vim {}", path.display());
    let targets = route(&command, &[]);
    assert_eq!(targets, vec![RoutedTarget::LauncherOperand { path }]);

    let resolution = resolve_for_analysis(
        targets.into_iter().next().unwrap(),
        AnalysisCwd::Unavailable,
        1024,
    )
    .await;
    assert!(matches!(resolution, Resolution::NotApplicable));
}

#[tokio::test]
async fn tar_missing_archive_operand_is_not_degraded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.tar");

    let command = format!("tar -xf {}", path.display());
    let targets = route(&command, &[]);
    assert_eq!(targets, vec![RoutedTarget::LauncherOperand { path }]);

    let resolution = resolve_for_analysis(
        targets.into_iter().next().unwrap(),
        AnalysisCwd::Unavailable,
        1024,
    )
    .await;
    assert!(matches!(resolution, Resolution::NotApplicable));
}

#[tokio::test]
async fn wget_missing_output_operand_is_not_degraded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out");

    let command = format!("wget -O {} http://x", path.display());
    let targets = route(&command, &[]);
    assert_eq!(targets, vec![RoutedTarget::LauncherOperand { path }]);

    let resolution = resolve_for_analysis(
        targets.into_iter().next().unwrap(),
        AnalysisCwd::Unavailable,
        1024,
    )
    .await;
    assert!(matches!(resolution, Resolution::NotApplicable));
}

#[tokio::test]
async fn zsh_missing_script_operand_is_not_degraded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.sh");

    let command = format!("zsh {}", path.display());
    let targets = route(&command, &[]);
    assert_eq!(targets, vec![RoutedTarget::LauncherOperand { path }]);

    let resolution = resolve_for_analysis(
        targets.into_iter().next().unwrap(),
        AnalysisCwd::Unavailable,
        1024,
    )
    .await;
    assert!(matches!(resolution, Resolution::NotApplicable));
}

#[tokio::test]
async fn code_directory_operand_is_not_degraded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("srcdir");
    std::fs::create_dir(&path).unwrap();

    let command = format!("code {}", path.display());
    let targets = route(&command, &[]);
    assert_eq!(targets, vec![RoutedTarget::LauncherOperand { path }]);

    let resolution = resolve_for_analysis(
        targets.into_iter().next().unwrap(),
        AnalysisCwd::Unavailable,
        1024,
    )
    .await;
    assert!(matches!(resolution, Resolution::NotApplicable));
}

#[tokio::test]
async fn du_directory_operand_is_not_degraded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("srcdir");
    std::fs::create_dir(&path).unwrap();

    let command = format!("du -sh {}", path.display());
    let targets = route(&command, &[]);
    assert_eq!(targets, vec![RoutedTarget::LauncherOperand { path }]);

    let resolution = resolve_for_analysis(
        targets.into_iter().next().unwrap(),
        AnalysisCwd::Unavailable,
        1024,
    )
    .await;
    assert!(matches!(resolution, Resolution::NotApplicable));
}

#[tokio::test]
async fn tree_directory_operand_is_not_degraded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("srcdir");
    std::fs::create_dir(&path).unwrap();

    let command = format!("tree {}", path.display());
    let targets = route(&command, &[]);
    assert_eq!(targets, vec![RoutedTarget::LauncherOperand { path }]);

    let resolution = resolve_for_analysis(
        targets.into_iter().next().unwrap(),
        AnalysisCwd::Unavailable,
        1024,
    )
    .await;
    assert!(matches!(resolution, Resolution::NotApplicable));
}

#[tokio::test]
async fn pytest_directory_operand_is_not_degraded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tests");
    std::fs::create_dir(&path).unwrap();

    let command = format!("pytest {}", path.display());
    let targets = route(&command, &[]);
    assert_eq!(targets, vec![RoutedTarget::LauncherOperand { path }]);

    let resolution = resolve_for_analysis(
        targets.into_iter().next().unwrap(),
        AnalysisCwd::Unavailable,
        1024,
    )
    .await;
    assert!(matches!(resolution, Resolution::NotApplicable));
}

#[tokio::test]
#[cfg(unix)]
async fn setsid_launcher_operand_naming_a_symlink_still_degrades() {
    // A symlink is neither "missing" nor a "literal directory" — the narrow
    // pair this fix carves out — so it keeps degrading exactly as a
    // user-typed direct-exec target's symlink operand already does
    // (`source_reader::read_script_file` rejects every symlink without
    // following it, regardless of what it points to).
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("real.py");
    std::fs::write(&target, "print(1)\n").unwrap();
    let link = dir.path().join("link.py");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let command = format!("setsid {}", link.display());
    let targets = route(&command, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::LauncherOperand { path: link.clone() }]
    );

    let resolution = resolve_for_analysis(
        targets.into_iter().next().unwrap(),
        AnalysisCwd::Unavailable,
        1024,
    )
    .await;
    assert!(matches!(
        resolution,
        Resolution::Degraded(DegradationReason::UnsafeSource)
    ));
}

// ── A path-like flag value does not shadow a real later target
// (issue #384/#430) ─────────────────────────────────────────────────

#[tokio::test]
async fn flag_value_operand_does_not_shadow_the_real_script_that_follows() {
    let dir = tempfile::tempdir().unwrap();
    let notes = dir.path().join("notes.txt");
    std::fs::write(&notes, "just notes, not a script\n").unwrap();
    let script = dir.path().join("pyx");
    std::fs::write(&script, "#!/usr/bin/env python3\nprint(1)\n").unwrap();

    let command = format!("setsid -u {} {}", notes.display(), script.display());
    let targets = route(&command, &[]);
    assert_eq!(
        targets,
        vec![
            RoutedTarget::LauncherOperand {
                path: notes.clone()
            },
            RoutedTarget::LauncherOperand {
                path: script.clone()
            },
        ]
    );

    let results = resolve(targets, 1024).await;
    assert_eq!(
        results,
        vec![Ok(SourceTarget {
            language: SourceLanguage::Python,
            source: "#!/usr/bin/env python3\nprint(1)\n".to_owned(),
        })]
    );
}

#[tokio::test]
async fn missing_flag_value_operand_does_not_shadow_the_real_script_that_follows() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.txt");
    let script = dir.path().join("pyx");
    std::fs::write(&script, "#!/usr/bin/env python3\nprint(1)\n").unwrap();

    let command = format!("setsid -u {} {}", missing.display(), script.display());
    let targets = route(&command, &[]);
    assert_eq!(
        targets,
        vec![
            RoutedTarget::LauncherOperand {
                path: missing.clone()
            },
            RoutedTarget::LauncherOperand {
                path: script.clone()
            },
        ]
    );

    let results = resolve(targets, 1024).await;
    assert_eq!(
        results,
        vec![Ok(SourceTarget {
            language: SourceLanguage::Python,
            source: "#!/usr/bin/env python3\nprint(1)\n".to_owned(),
        })]
    );
}

// ── Everyday multi-operand shapes stay unrouted (issue #384/#430) ──

#[test]
fn cp_of_two_plain_files_is_not_routed() {
    assert_eq!(route("cp ./a.txt ./b.txt", &[]), Vec::new());
}

#[test]
fn diff_of_two_plain_files_is_not_routed() {
    assert_eq!(route("diff ./notes.txt ./file.txt", &[]), Vec::new());
}

#[tokio::test]
async fn tar_create_of_a_plain_directory_is_not_degraded() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("out.tar");
    let src = dir.path().join("srcdir");
    std::fs::create_dir(&src).unwrap();

    let command = format!("tar -cf {} {}", archive.display(), src.display());
    let targets = route(&command, &[]);
    assert_eq!(
        targets,
        vec![
            RoutedTarget::LauncherOperand {
                path: archive.clone()
            },
            RoutedTarget::LauncherOperand { path: src.clone() },
        ]
    );

    let results = resolve(targets, 1024).await;
    assert_eq!(results, Vec::new());
}

// ── A program word reached only through expansion the router does not
// perform still degrades when it carries an operand (issue #384/#430) ──────

#[test]
fn variable_holding_an_interpreter_name_is_routed() {
    assert_eq!(
        route("VAR=python3; $VAR ./evil.py", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn parameter_expansion_default_is_routed() {
    assert_eq!(
        route("${X:-python3} ./evil.py", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn brace_list_program_word_is_routed() {
    assert_eq!(
        route("{python3,} ./evil.py", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn alias_defined_earlier_on_the_line_is_routed() {
    assert_eq!(
        route("alias runpy=python3; runpy ./evil.py", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn alias_defined_with_a_double_dash_separator_is_routed() {
    assert_eq!(
        route("alias -- n=python3; n ./evil.py", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn alias_defined_among_several_names_in_one_call_is_routed() {
    assert_eq!(
        route("alias a=ls b=python3; b ./evil.py", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn quoted_alias_name_is_routed() {
    assert_eq!(
        route("alias 'n'=python3; n ./evil.py", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn alias_replacement_carrying_interpreter_flags_is_routed() {
    assert_eq!(
        route("alias n='python3 -u'; n ./evil.py", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn alias_replacement_with_a_leading_assignment_is_routed() {
    assert_eq!(
        route("alias n='X=1 python3'; n ./evil.py", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn alias_replacement_naming_a_variable_is_routed() {
    assert_eq!(
        route(r#"alias n="$X"; n ./evil.py"#, &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn a_redefined_alias_resolves_to_its_latest_definition() {
    // review comment 4091038665: `run` is redefined from the benign `echo`
    // to `python3` before its call site, so the definition active there —
    // the last one before use — is the one that must be seen.
    assert_eq!(
        route("alias run=echo; alias run=python3; run ./evil.py", &[]),
        vec![unresolved_dynamic()]
    );
}

// ── An alias standing in for an ordinary (non-interpreter) command is left
// to the rest of routing, not degraded by name alone (issue #384/#430)
// ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn alias_for_a_plain_command_naming_a_directory_operand_is_not_degraded() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    std::fs::create_dir(&src).unwrap();

    let command = format!("alias ll='ls -l'; ll {}", src.display());
    let targets = route(&command, &[]);
    assert_eq!(targets, vec![RoutedTarget::LauncherOperand { path: src }]);

    let results = resolve(targets, 1024).await;
    assert_eq!(results, Vec::new());
}

#[test]
fn alias_for_a_plain_command_with_no_path_like_operand_is_not_routed() {
    assert_eq!(route("alias g=git; g status", &[]), Vec::new());
}

#[test]
fn a_redefined_alias_naming_a_plain_command_last_is_not_routed() {
    // The reverse of `a_redefined_alias_resolves_to_its_latest_definition`:
    // redefined away from an interpreter, the latest definition must win
    // just as much when it clears the flag as when it sets it.
    assert_eq!(
        route("alias run=python3; alias run=git; run status", &[]),
        Vec::new()
    );
}

// ── A static program word with a dynamic operand, or a bare dynamic word
// with no operand at all, stays exactly as auto-approved as before ─────────

#[test]
fn echo_of_a_variable_is_not_routed() {
    assert_eq!(route("echo $HOME", &[]), Vec::new());
}

#[test]
fn ls_of_a_brace_list_operand_is_not_routed() {
    assert_eq!(route("ls {a,b}", &[]), Vec::new());
}

#[test]
fn bare_variable_with_no_operand_is_not_routed() {
    // `$EDITOR` alone opens an interactive editor with no arguments of its
    // own — the same everyday shape as `vim` with no file — so there is no
    // operand for this net to treat as a launched target (issue #384/#430).
    assert_eq!(route("$EDITOR", &[]), Vec::new());
}

// ── A second unenumerated wrapper word is walked past to reach the real
// operand (issue #384/#430) ─────────────────────────────────────────────────

#[tokio::test]
async fn double_wrapped_interpreter_is_routed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pyx");
    std::fs::write(&path, "#!/usr/bin/env python3\nprint(1)\n").unwrap();

    let command = format!("strace setsid {}", path.display());
    let targets = route(&command, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::LauncherOperand { path: path.clone() }]
    );

    let results = resolve(targets, 1024).await;
    assert_eq!(
        results,
        vec![Ok(SourceTarget {
            language: SourceLanguage::Python,
            source: "#!/usr/bin/env python3\nprint(1)\n".to_owned(),
        })]
    );
}

#[test]
fn wrapper_flags_own_argument_is_not_mistaken_for_the_operand() {
    assert_eq!(
        route("setsid -u user ./pyx", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

// ── A user-typed direct-exec target keeps its pre-existing behavior ────────

#[tokio::test]
async fn user_typed_direct_exec_of_a_missing_script_still_degrades() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.sh");

    let command = path.display().to_string();
    let targets = route(&command, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::DirectExec { path: path.clone() }]
    );

    let resolution = resolve_for_analysis(
        targets.into_iter().next().unwrap(),
        AnalysisCwd::Unavailable,
        1024,
    )
    .await;
    assert!(matches!(
        resolution,
        Resolution::Degraded(DegradationReason::UnsafeSource)
    ));
}
