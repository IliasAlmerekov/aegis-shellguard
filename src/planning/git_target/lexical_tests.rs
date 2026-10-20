//! Lexical folding and the shell's logical starting directory (#525, third
//! Codex review of PR #529).

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use super::{normalize_lexically, start_dir};

#[test]
fn consecutive_leading_parents_of_a_relative_path_are_kept() {
    assert_eq!(
        normalize_lexically(Path::new("../../repo")),
        PathBuf::from("../../repo")
    );
}

#[test]
fn a_parent_after_a_leading_parent_does_not_cancel_it() {
    assert_eq!(
        normalize_lexically(Path::new("./a/../..")),
        PathBuf::from("..")
    );
}

#[test]
fn a_parent_folds_a_named_component() {
    assert_eq!(
        normalize_lexically(Path::new("/outer/nested/link/..")),
        PathBuf::from("/outer/nested")
    );
}

#[test]
fn a_parent_at_the_root_stays_at_the_root() {
    assert_eq!(normalize_lexically(Path::new("/..")), PathBuf::from("/"));
}

struct Links {
    _root: tempfile::TempDir,
    real: PathBuf,
    link: PathBuf,
}

fn links() -> Links {
    let root = tempfile::TempDir::new().unwrap();
    let base = root.path().canonicalize().unwrap();
    let real = base.join("real");
    let link = base.join("link");
    std::fs::create_dir(&real).unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();
    Links {
        _root: root,
        real,
        link,
    }
}

#[test]
fn a_valid_logical_pwd_is_the_starting_directory() {
    let fx = links();

    assert_eq!(start_dir(&fx.real, Some(fx.link.as_os_str())), fx.link);
}

#[test]
fn a_pwd_naming_another_directory_is_ignored() {
    let fx = links();
    let other = fx.real.parent().unwrap().to_path_buf();

    assert_eq!(start_dir(&fx.real, Some(other.as_os_str())), fx.real);
}

#[test]
fn a_relative_pwd_is_ignored() {
    let fx = links();

    assert_eq!(start_dir(&fx.real, Some(OsStr::new("link"))), fx.real);
}

#[test]
fn a_pwd_with_dot_components_is_ignored() {
    let fx = links();
    let dotted = fx.link.join(".");

    assert_eq!(start_dir(&fx.real, Some(dotted.as_os_str())), fx.real);
}

#[test]
fn no_pwd_keeps_the_physical_directory() {
    let fx = links();

    assert_eq!(start_dir(&fx.real, None), fx.real);
}
