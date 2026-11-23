//! Run a test with its workspace as the child process's working directory.

#[path = "isolated_test.rs"]
mod isolated_test;

use std::path::Path;

/// Set up a temporary workspace and run the current test in a child there.
pub fn run(setup: impl FnOnce(&Path), test: impl FnOnce(&Path)) {
    const CHILD: &str = "AEGIS_PROCESS_STATE_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let _completion = isolated_test::Completion;
        test(&std::env::current_dir().unwrap());
    } else {
        let workspace = tempfile::tempdir().unwrap();
        setup(workspace.path());
        let mut command = isolated_test::command();
        command.env(CHILD, "1").current_dir(workspace.path());
        isolated_test::run(&mut command);
    }
}
