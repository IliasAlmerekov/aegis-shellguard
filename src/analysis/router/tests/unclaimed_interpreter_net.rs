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

fn unresolved_dynamic() -> RoutedTarget {
    RoutedTarget::Unresolved {
        reason: DegradationReason::DynamicSource,
    }
}

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

// ── A path-like operand after an unenumerated launcher is a direct-exec candidate ──

#[test]
fn setsid_direct_exec_operand_is_routed() {
    assert_eq!(
        route("setsid ./pyx", &[]),
        vec![RoutedTarget::DirectExec {
            path: PathBuf::from("./pyx"),
        }]
    );
}

#[tokio::test]
async fn setsid_direct_exec_operand_with_a_verified_shebang_prompts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pyx");
    std::fs::write(&path, "#!/usr/bin/env python3\nprint(1)\n").unwrap();

    let command = format!("setsid {}", path.display());
    let targets = route(&command, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::DirectExec { path: path.clone() }]
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
async fn setsid_direct_exec_operand_without_a_shebang_stays_auto_approved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.txt");
    std::fs::write(&path, "just notes, not a script\n").unwrap();

    let command = format!("setsid {}", path.display());
    let targets = route(&command, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::DirectExec { path: path.clone() }]
    );

    let results = resolve(targets, 1024).await;
    assert_eq!(results, Vec::new());
}
