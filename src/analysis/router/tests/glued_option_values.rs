//! Glued short-flag executor values (GHSA-xj54 follow-up): a value
//! glued directly onto a short-flag bundle's own letters (`tar -I./pyx`,
//! `rsync -e./pyx`, `man -P./pyx`) used to vanish entirely — the unclaimed
//! net's own operand walk only reads a `-`-prefixed token when it carries an
//! `=`, so a flag with a value glued straight on with no `=` at all was
//! silently skipped. `use super::*` reaches the same `router` test imports
//! (`RoutedTarget`, `route`, `unresolved_dynamic`, ...) its sibling files use.

use super::*;

// ── A glued short-flag path value routes as its own launcher operand, same
// as the already-supported spaced form ─────────────────────────────────────

#[test]
fn tar_glued_compress_program_at_the_start_of_the_group_routes() {
    assert_eq!(
        route("tar -I./pyx -cf out.tar dir", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

#[test]
fn tar_glued_compress_program_after_other_flags_routes() {
    assert_eq!(
        route("tar -cf out.tar -I./pyx dir", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

#[test]
fn tar_glued_compress_program_bundled_with_another_short_flag_routes() {
    // `-cI./pyx` is `-c` `-I./pyx` bundled into one token: the value is glued
    // onto the *second* flag letter, not the first, so every split up to the
    // flag-letter run must be tried, not only the one right after `-`. That
    // also tries split=1 (`I./pyx`), a letter run consumed as if `-cI` were
    // itself one single-letter-per-flag bundle — a spurious extra candidate
    // routing cannot rule out without knowing tar's own flag grammar, so it
    // shows up in `route()` too, alongside the real `./pyx` split=2. It is
    // harmless: `I./pyx` names no real file, so a caller resolving it finds
    // nothing there, same as any other nonexistent-path operand — asserted
    // here by presence, not exact `route()` equality, matching the other
    // "extra candidate that resolves to nothing" shapes this file covers.
    let targets = route("tar -cI./pyx -f out.tar dir", &[]);
    assert!(
        targets.contains(&RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }),
        "{targets:?}"
    );
}

#[test]
fn tar_glued_compress_program_naming_an_interpreter_degrades() {
    // Same interpreter-carrying value `tar -I 'python3 ./evil.py' -cf ...`
    // already degrades on, just glued: the value's own leading word
    // (`python3`) is itself several letters, so the flag-letter run search
    // must stop at the *first* split that resolves, not swallow part of the
    // interpreter name into the guessed flag-letter prefix.
    assert_eq!(
        route("tar -I'python3 ./evil.py' -cf out.tar dir", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn rsync_glued_remote_shell_path_routes() {
    assert_eq!(
        route("rsync -e./pyx a h:b", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

#[test]
fn rsync_glued_remote_shell_naming_an_interpreter_degrades() {
    assert_eq!(
        route("rsync -e'python3 ./evil.py' a h:b", &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn man_glued_pager_path_routes() {
    assert_eq!(
        route("man -P./pyx ls", &[]),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

#[test]
fn man_glued_pager_naming_an_interpreter_degrades() {
    assert_eq!(
        route("man -P'python3 ./evil.py' ls", &[]),
        vec![unresolved_dynamic()]
    );
}

// ── A nested `NAME=value` executor value unwraps again when the value
// itself still carries a `NAME=` prefix ────────────────────────────────────

#[test]
fn tar_checkpoint_action_exec_value_routes_its_unwrapped_path() {
    assert_eq!(
        route(
            "tar --checkpoint=1 --checkpoint-action=exec=./pyx -cf out.tar dir",
            &[]
        ),
        vec![RoutedTarget::LauncherOperand {
            path: PathBuf::from("./pyx"),
        }]
    );
}

#[test]
fn tar_checkpoint_action_exec_value_naming_an_interpreter_degrades() {
    assert_eq!(
        route(
            "tar --checkpoint=1 --checkpoint-action=exec='python3 ./evil.py' -cf out.tar dir",
            &[]
        ),
        vec![unresolved_dynamic()]
    );
}

// ── A glued value that is a regular file with no shebang gives no target,
// same as the equivalent bare operand ──────────────────────────────────────

#[tokio::test]
async fn a_glued_value_naming_a_shebang_less_file_yields_no_target() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.txt");
    std::fs::write(&path, "just data\n").unwrap();

    let command = format!("tar -o{} -cf out.tar dir", path.display());
    let targets = route(&command, &[]);
    assert_eq!(
        targets,
        vec![RoutedTarget::LauncherOperand { path: path.clone() }]
    );

    let results = resolve(targets, 1024).await;
    assert_eq!(results, Vec::new());
}

// ── A glued `$`/glob value is an accepted false positive: it degrades
// rather than being read as a literal path ─────────────────────────────────

#[test]
fn a_glued_dollar_or_glob_value_degrades_an_accepted_false_positive() {
    assert_eq!(
        route("tar -I$HOME/inc -cf out.tar dir", &[]),
        vec![unresolved_dynamic()]
    );
}

// ── Ordinary commands give no target once resolved: a bare flag bundle with
// nothing glued on, a value with no path, a program on the data-only
// exclusion list, or a route()-level candidate that resolves to nothing
// because it names a directory or a real-but-missing path (the "test trap":
// several of these DO produce a `LauncherOperand` at route() level; they are
// safe only because resolve() finds a directory or a missing path, issue
// #384/#430, GHSA-xj54) ─────────────────────────────────────────────────────

#[tokio::test]
async fn ordinary_glued_short_flags_and_unaffected_shapes_give_no_target() {
    for command in [
        "gcc -I/usr/include -c a.c",
        "cc -L./lib -o a a.c",
        "tar -czf backup.tar.gz dir",
        "tar -xzf x.tgz -C /tmp/x",
        "man -P cat ls",
        "ssh -o ConnectTimeout=5 host",
        "grep -rn TODO src/",
        "sort -o out.txt in.txt",
        "head -n5 ./a.txt",
        "docker compose -f dir/compose.yml ps",
        "ls -la ~/.config",
    ] {
        let results = resolve(route(command, &[]), 1024).await;
        assert_eq!(results, Vec::new(), "{command}");
    }
}

// ── A glued short-flag value that happens to start with an interpreter's
// name is a match only when the value carries more than one word: a linker
// or include flag's own argument (`-lpython3.12`, `-lnode`, `-Ipython3`) is
// one word and never runs anything, unlike a quoted multi-word value
// (`-I'python3 ./evil.py'`) that hands the interpreter a real command line
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn a_glued_single_word_interpreter_name_from_a_linker_or_include_flag_auto_approves() {
    for command in [
        "gcc -lpython3.12 -o a a.c",
        "cc -lnode a.c",
        "gcc -Ipython3 -c a.c",
    ] {
        assert_eq!(route(command, &[]), Vec::new(), "{command}");
    }
}

#[test]
fn a_glued_multi_word_interpreter_value_still_degrades() {
    for command in [
        "tar -I'python3 ./evil.py' -cf out.tar dir",
        "rsync -e'python3 ./evil.py' a h:b",
        "man -P'python3 ./evil.py' ls",
    ] {
        assert_eq!(route(command, &[]), vec![unresolved_dynamic()], "{command}");
    }
}
