use std::path::Path;
use std::process::Command;

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
