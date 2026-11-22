//! The one way tests spawn `git`. Unit tests in `src/` reach it as
//! `crate::test_git`, integration tests through `tests/support`.

use std::path::Path;
use std::process::Command;

/// `git` running in `dir` with every inherited `GIT_` variable removed. Git
/// exports `GIT_DIR` to hooks in a linked worktree, so without this a test run
/// from a hook writes into the developer's repository.
pub(crate) fn git_command(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(dir);
    for (key, _) in std::env::vars_os() {
        if key.as_encoded_bytes().starts_with(b"GIT_") {
            command.env_remove(key);
        }
    }
    command
}

/// Creates `path` if needed and runs `git init -q` in it.
pub(crate) fn git_init(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    assert!(
        git_command(path)
            .args(["init", "-q"])
            .status()
            .unwrap()
            .success()
    );
}
