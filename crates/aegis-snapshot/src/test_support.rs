//! Test-only helpers shared by the snapshot plugins' fake-executable tests.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Write `contents` to `path` and make it executable, without this process
/// ever holding an open write handle on `path`.
///
/// With `fs::write`, another test thread can `fork()` while this process
/// holds the write descriptor. The child keeps a copy of it until its own
/// `exec()` closes it. If the test runs the fake executable inside that
/// window, `execve` fails with `ETXTBSY` ("Text file busy") (#436). Renaming
/// the file afterwards does not help, because the copied descriptor still
/// points at the same inode. A short-lived child process does the write
/// here, so no descriptor of this process ever refers to the file.
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

/// Write a fake `name` executable under `dir` whose body is `body`, wrapped
/// in a `#!/bin/sh` script with `set -eu`. Returns the path to the stub.
pub(crate) fn stub_bin(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    write_executable(&path, &format!("#!/bin/sh\nset -eu\n{body}\n"));
    path
}
