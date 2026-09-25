//! Test-only helpers shared by the snapshot plugins' fake-executable tests.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Write `contents` to `path` and make it executable, without this process
/// ever holding an open write handle on `path`.
///
/// A test thread that writes the file with `fs::write` (open, write, close)
/// races every other test thread that spawns a child process: on Linux, a
/// `fork()` between this process's `open()` and `close()` duplicates the
/// still-open write file descriptor into the child, which then inherits it
/// across its own `exec()` until it closes on exit. If a *different* test
/// spawns a fake executable at the same path in that window, the kernel
/// rejects the `execve` with `ETXTBSY` ("Text file busy") because a write fd
/// on the file is still open somewhere in the process tree. Renaming the
/// file after writing does not help: a rename does not change the inode a
/// live fd points at.
///
/// Doing the write from a short-lived child process instead means no fd of
/// *this* process ever refers to the file, so this process contributes
/// nothing to another thread's `fork()` snapshot.
pub(crate) fn write_executable(path: &Path, contents: &str) {
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
