mod support;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use support::run_git;
use tempfile::TempDir;

fn run_text(root: &Path) -> Output {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/lint.sh");
    Command::new("sh")
        .arg(script)
        .arg("text")
        .env("LINT_TEXT_ROOT", root)
        .output()
        .expect("sh should run scripts/lint.sh")
}

fn write_file(root: &Path, rel: &str, contents: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn write_lines(root: &Path, rel: &str, lines: usize, trailing_newline: bool) {
    let mut body = "// line\n".repeat(lines);
    if !trailing_newline {
        body.pop();
    }
    write_file(root, rel, &body);
}

fn stdout(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn text_fails_on_801_line_rust_file_and_names_it() {
    let tree = TempDir::new().unwrap();
    write_lines(tree.path(), "tests/big.rs", 801, true);
    let output = run_text(tree.path());
    assert!(!output.status.success());
    assert!(
        stdout(&output).contains("tests/big.rs"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn text_counts_a_last_line_without_trailing_newline() {
    let tree = TempDir::new().unwrap();
    write_lines(tree.path(), "src/big.rs", 801, false);
    let output = run_text(tree.path());
    assert!(!output.status.success());
    assert!(
        stdout(&output).contains("src/big.rs"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn text_passes_an_800_line_rust_file() {
    let tree = TempDir::new().unwrap();
    write_lines(tree.path(), "tests/exact.rs", 800, true);
    let output = run_text(tree.path());
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn text_skips_pruned_directories_at_any_depth() {
    let tree = TempDir::new().unwrap();
    write_lines(tree.path(), "fuzz/target/gen.rs", 900, true);
    write_lines(tree.path(), "crates/x/target/gen.rs", 900, true);
    let output = run_text(tree.path());
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn text_reports_every_offender() {
    let tree = TempDir::new().unwrap();
    write_lines(tree.path(), "src/a.rs", 801, true);
    write_lines(tree.path(), "benches/b.rs", 850, true);
    let output = run_text(tree.path());
    let text = stdout(&output);
    assert!(!output.status.success());
    assert!(
        text.contains("src/a.rs") && text.contains("benches/b.rs"),
        "{text}"
    );
}

#[test]
fn text_passes_an_empty_rust_file() {
    let tree = TempDir::new().unwrap();
    write_lines(tree.path(), "src/empty.rs", 0, true);
    let output = run_text(tree.path());
    assert!(output.status.success(), "{}", stdout(&output));
}

#[cfg(unix)]
mod unreadable {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn chmod(path: &Path, mode: u32) {
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn text_fails_on_an_unreadable_rust_file_and_names_it() {
        let tree = TempDir::new().unwrap();
        write_lines(tree.path(), "src/locked.rs", 801, true);
        let locked = tree.path().join("src/locked.rs");
        chmod(&locked, 0o000);
        let output = run_text(tree.path());
        chmod(&locked, 0o644);
        let text = stdout(&output);
        assert!(!output.status.success(), "{text}");
        assert!(text.contains("src/locked.rs"), "{text}");
    }

    #[test]
    fn text_fails_on_an_unreadable_directory_and_names_it() {
        let tree = TempDir::new().unwrap();
        write_lines(tree.path(), "src/sealed/a.rs", 1, true);
        let sealed = tree.path().join("src/sealed");
        chmod(&sealed, 0o000);
        let output = run_text(tree.path());
        chmod(&sealed, 0o755);
        let text = stdout(&output);
        assert!(!output.status.success(), "{text}");
        assert!(text.contains("src/sealed"), "{text}");
    }
}

#[test]
fn text_fails_on_missing_backticked_path_and_names_file_and_path() {
    let tree = TempDir::new().unwrap();
    write_file(
        tree.path(),
        "CONVENTION.md",
        "See `src/does_not_exist.rs` and `MISSING.md`.\n",
    );
    let output = run_text(tree.path());
    let text = stdout(&output);
    assert!(!output.status.success(), "{text}");
    assert!(
        text.contains("CONVENTION.md")
            && text.contains("src/does_not_exist.rs")
            && text.contains("MISSING.md"),
        "{text}"
    );
}

#[test]
fn text_passes_when_backticked_paths_exist_in_checked_docs() {
    let tree = TempDir::new().unwrap();
    write_file(tree.path(), "Cargo.toml", "[package]\nname = \"x\"\n");
    write_file(tree.path(), "docs/guide.md", "# Guide\n");
    write_file(tree.path(), "scripts/check.sh", "#!/bin/sh\n");
    write_file(tree.path(), ".github/workflows/ci.yml", "name: ci\n");
    write_file(tree.path(), "src/main.rs", "fn main() {}\n");
    for doc in ["AGENTS.md", "CONVENTION.md", "CONTRIBUTING.md", "CLAUDE.md"] {
        write_file(
            tree.path(),
            doc,
            "Read `Cargo.toml`, `docs/guide.md`, `scripts/check.sh`, \
             `.github/workflows/ci.yml`, `src/main.rs`, and `docs/`.\n",
        );
    }
    let output = run_text(tree.path());
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn text_skips_globs_placeholders_home_templates_bare_ext_slashes_and_allowlist() {
    let tree = TempDir::new().unwrap();
    write_file(
        tree.path(),
        "AGENTS.md",
        "Globs: `crates/*/src/**`, `tests/foo?.rs`, `src/[ab].rs`.\n\
         Placeholders: `docs/<topic>.md`, `<appDataDir>/brain/<id>/notes.md`.\n\
         Home: `~/.agents/skills/`, `~/.config/aegis/config.toml`, `~/.aegis/audit.jsonl`.\n\
         Template: `docs/guide/guide-NNN-slug.md`.\n\
         Bare extensions: `.rs`, `.md`, `.toml`, `.yml`, `.sh`.\n\
         Slash-only: `///`, `/`.\n\
         Allowlist: `.aegis.toml`.\n",
    );
    let output = run_text(tree.path());
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn text_skips_gitignored_backticked_paths() {
    let tree = TempDir::new().unwrap();
    run_git(tree.path(), &["init", "-q"]);

    write_file(
        tree.path(),
        ".gitignore",
        "RTK.md\n.claude/settings.local.json\ntarget/\n",
    );
    write_file(
        tree.path(),
        "AGENTS.md",
        "Local files: `RTK.md`, `.claude/settings.local.json`, `target/`.\n",
    );
    let output = run_text(tree.path());
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn text_catches_missing_path_adjacent_to_multiword_backtick_span() {
    let tree = TempDir::new().unwrap();
    write_file(
        tree.path(),
        "AGENTS.md",
        "Run `cargo test`,`src/missing.rs`.\n",
    );
    let output = run_text(tree.path());
    let text = stdout(&output);
    assert!(!output.status.success(), "{text}");
    assert!(
        text.contains("AGENTS.md") && text.contains("src/missing.rs"),
        "{text}"
    );
}

#[test]
fn text_does_not_skip_unpaired_bracket_as_glob() {
    let tree = TempDir::new().unwrap();
    write_file(tree.path(), "CONVENTION.md", "See `docs/draft[.md`.\n");
    let output = run_text(tree.path());
    let text = stdout(&output);
    assert!(!output.status.success(), "{text}");
    assert!(
        text.contains("CONVENTION.md") && text.contains("docs/draft[.md"),
        "{text}"
    );
}

#[test]
fn text_fails_on_path_escaping_lint_text_root() {
    let parent = TempDir::new().unwrap();
    write_file(parent.path(), "outside.md", "# Outside\n");
    let tree = parent.path().join("tree");
    write_file(&tree, "AGENTS.md", "See `../outside.md`.\n");
    let output = run_text(&tree);
    let text = stdout(&output);
    assert!(!output.status.success(), "{text}");
    assert!(
        text.contains("AGENTS.md") && text.contains("../outside.md"),
        "{text}"
    );
}

#[test]
fn text_fails_when_tamper_check_self_test_fails() {
    let tree = TempDir::new().unwrap();
    write_file(
        tree.path(),
        "scripts/test-tamper-check.test.sh",
        "#!/usr/bin/env bash\necho \"FAIL synthetic tamper case\"\nexit 1\n",
    );
    let output = run_text(tree.path());
    let text = stdout(&output);
    assert!(!output.status.success(), "{text}");
    assert!(text.contains("FAIL synthetic tamper case"), "{text}");
}

#[test]
fn text_passes_when_tamper_check_self_test_passes() {
    let tree = TempDir::new().unwrap();
    write_file(
        tree.path(),
        "scripts/test-tamper-check.test.sh",
        "#!/usr/bin/env bash\nexit 0\n",
    );
    let output = run_text(tree.path());
    assert!(output.status.success(), "{}", stdout(&output));
}

const GIT_SPAWN: &str = concat!("let _ = std::process::Command::new(", "\"git\");\n");

#[test]
fn text_fails_on_git_spawn_outside_the_shared_helpers_and_names_each() {
    let tree = TempDir::new().unwrap();
    let offenders = [
        "tests/plain.rs",
        "tests/support/extra.rs",
        "crates/aegis-x/tests/it.rs",
        "src/lib_tests.rs",
        "crates/aegis-x/src/lib.rs",
    ];
    for rel in offenders {
        write_file(tree.path(), rel, GIT_SPAWN);
    }
    let output = run_text(tree.path());
    assert!(!output.status.success());
    let report = stdout(&output);
    for rel in offenders {
        assert!(report.contains(rel), "{rel} missing from:\n{report}");
    }
}

#[test]
fn text_passes_git_spawn_in_the_shared_helpers() {
    let tree = TempDir::new().unwrap();
    write_file(tree.path(), "src/test_git.rs", GIT_SPAWN);
    write_file(
        tree.path(),
        "crates/aegis-snapshot/src/git/mod.rs",
        GIT_SPAWN,
    );
    let output = run_text(tree.path());
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn text_fails_on_tracker_id_rust_filenames_and_names_each() {
    let tree = TempDir::new().unwrap();
    let offenders = [
        "crates/x/src/tests/h9_demo.rs",
        "tests/foo_m7.rs",
        "src/c3_foo.rs",
        "crates/y/src/foo_c12_bar.rs",
    ];
    for rel in offenders {
        write_file(tree.path(), rel, "// test\n");
    }
    let output = run_text(tree.path());
    assert!(!output.status.success(), "{}", stdout(&output));
    let report = stdout(&output);
    for rel in offenders {
        assert!(report.contains(rel), "{rel} missing from:\n{report}");
    }
}

#[test]
fn text_passes_allowed_non_tracker_id_rust_filenames() {
    let tree = TempDir::new().unwrap();
    for rel in [
        "crates/aegis-language/src/l1_qualification_foo.rs",
        "tests/audit_v2.rs",
        "tests/normal_test.rs",
        "crates/x/target/h9_gen.rs",
    ] {
        write_file(tree.path(), rel, "// test\n");
    }
    let output = run_text(tree.path());
    assert!(output.status.success(), "{}", stdout(&output));
}

#[test]
fn text_fails_on_review_citation_comments_in_rust_and_toml_and_names_each() {
    let tree = TempDir::new().unwrap();
    let offenders = [
        ("src/a.rs", "// R1-2: drop this\n"),
        ("crates/x/src/b.rs", "/// See #437 review for details.\n"),
        ("tests/c.rs", "//! finding F1\n"),
        ("benches/d.rs", "    // finding 3\n"),
        ("tests/fixtures/e.toml", "# PR #463 review: reason\n"),
        ("crates/x/f.toml", "  # R2-10 note\n"),
        ("src/g.toml", "# finding A12\n"),
    ];
    for (rel, body) in offenders {
        write_file(tree.path(), rel, body);
    }
    let output = run_text(tree.path());
    let report = stdout(&output);
    assert!(!output.status.success(), "{report}");
    assert!(report.contains("CONVENTION.md"), "{report}");
    for (rel, _) in offenders {
        assert!(report.contains(rel), "{rel} missing from:\n{report}");
    }
}

#[test]
fn text_passes_clean_comments_and_non_comment_review_patterns() {
    let tree = TempDir::new().unwrap();
    let clean_rs = [
        "let x = 1; // R1-2 trailing comment",
        "// r1-2 lowercase",
        "// Finding F1 wrong case",
        "// #437 Review wrong case",
        "// finding f1 lowercase letter",
        "// VAR1-2 not a review round",
        "// see #437",
        "#[doc = \"#437 review finding F1 R1-2\"]",
    ]
    .join("\n");
    write_file(tree.path(), "src/clean.rs", &clean_rs);

    let clean_toml = [
        "key = \"val\" # #437 review trailing",
        "# r1-2 lowercase",
        "# Finding F1 wrong case",
        "# #437 Review wrong case",
        "# finding f1 lowercase letter",
        "# STR1-2 not a review round",
        "# see #463",
        "note = \"// R1-2\"",
    ]
    .join("\n");
    write_file(tree.path(), "tests/fixtures/clean.toml", &clean_toml);

    write_file(tree.path(), "crates/x/target/gen.rs", "// R1-2\n");
    write_file(tree.path(), "tests/target/gen.toml", "# #437 review\n");
    let output = run_text(tree.path());
    assert!(output.status.success(), "{}", stdout(&output));
}
