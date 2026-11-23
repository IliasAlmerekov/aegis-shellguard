//! Run a test with its workspace as the child process's working directory.

use std::path::Path;

/// Set up a temporary workspace and run the current test in a child there.
pub fn run(setup: impl FnOnce(&Path), test: impl FnOnce(&Path)) {
    const CHILD: &str = "AEGIS_PROCESS_STATE_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        test(&std::env::current_dir().unwrap());
    } else {
        let workspace = tempfile::tempdir().unwrap();
        setup(workspace.path());
        let name = std::thread::current().name().unwrap().to_owned();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &name, "--nocapture"])
            .env(CHILD, "1")
            .current_dir(workspace.path())
            .status()
            .unwrap();
        assert!(status.success());
    }
}
