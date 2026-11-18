//! `scripts/lint.sh text` run against fixture trees, never the checkout.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

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

fn write_lines(root: &Path, rel: &str, lines: usize, trailing_newline: bool) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut body = "// line\n".repeat(lines);
    if !trailing_newline {
        body.pop();
    }
    fs::write(path, body).unwrap();
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

// shortcut: root ignores mode bits, so these fail when run as root; add a skip if CI ever runs tests as root.
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
