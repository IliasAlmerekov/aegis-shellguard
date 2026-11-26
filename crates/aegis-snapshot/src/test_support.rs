use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn write_executable(path: &Path, contents: &str) {
    let mut child = Command::new("/bin/sh")
        .arg("-c")
        .arg(r#"cat > "$1" && chmod 755 "$1""#)
        .arg("sh")
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
        .expect("failed to spawn helper process to write executable");

    child
        .stdin
        .take()
        .expect("child stdin must be piped")
        .write_all(contents.as_bytes())
        .expect("failed to write executable contents to helper process");

    let status = child.wait().expect("failed to wait for helper process");
    assert!(status.success(), "failed to write executable at {path:?}");
}

pub(crate) fn stub_bin(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    write_executable(&path, &format!("#!/bin/sh\nset -eu\n{body}\n"));
    path
}
