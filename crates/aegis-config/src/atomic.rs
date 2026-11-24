use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Atomically write `bytes` to `path` via a sibling temporary file and rename.
///
/// Creates a unique temporary file in `path`'s parent directory using
/// `create_new`, applies `mode` on Unix when `Some` (ignoring the process
/// umask), writes `bytes`, calls `sync_all`, and renames the temporary file
/// over `path`. Removes the temporary file on any failure after it is created
/// and returns the original I/O error. Does not create the parent directory.
pub fn write_atomic(path: &Path, bytes: &[u8], mode: Option<u32>) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "path has no parent directory")
    })?;

    let temp_path = temporary_path(parent);
    let mut open_options = OpenOptions::new();
    open_options.write(true).create_new(true);
    #[cfg(unix)]
    if let Some(mode) = mode {
        use std::os::unix::fs::OpenOptionsExt;
        open_options.mode(mode);
    }
    #[cfg(not(unix))]
    {
        // File permission modes are Unix-only; non-Unix targets keep default permissions.
        let _ = mode;
    }

    let mut file = open_options.open(&temp_path)?;
    let write_result = (|| -> io::Result<()> {
        #[cfg(unix)]
        if let Some(mode) = mode {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(mode))?;
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    })();
    drop(file);

    let result = write_result.and_then(|()| fs::rename(&temp_path, path));
    if result.is_err() {
        // Best-effort temp cleanup; preserve the primary I/O error.
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn temporary_path(parent: &Path) -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);

    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);

    parent.join(format!(".aegis-{pid}-{nanos}-{seq}.tmp"))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use tempfile::TempDir;

    use super::*;

    fn dir_entries(dir: &Path) -> Vec<PathBuf> {
        fs::read_dir(dir)
            .expect("read dir")
            .map(|entry| entry.expect("dir entry").path())
            .collect()
    }

    #[test]
    fn write_atomic_creates_and_overwrites_file_without_leaving_temp_files() {
        let dir = TempDir::new().expect("temp dir");
        let target = dir.path().join("config.toml");

        write_atomic(&target, b"first payload\n", None).expect("initial write");
        assert_eq!(fs::read(&target).expect("read initial"), b"first payload\n");
        assert_eq!(dir_entries(dir.path()), vec![target.clone()]);

        write_atomic(&target, b"second", None).expect("overwrite");
        assert_eq!(fs::read(&target).expect("read overwritten"), b"second");
        assert_eq!(dir_entries(dir.path()), vec![target]);
    }

    #[cfg(unix)]
    #[test]
    fn write_atomic_applies_exact_unix_modes() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().expect("temp dir");

        let hook = dir.path().join("hook.sh");
        write_atomic(&hook, b"#!/bin/sh\n", Some(0o755)).expect("write 0755");
        let hook_mode = fs::metadata(&hook).expect("stat hook").permissions().mode() & 0o7777;
        assert_eq!(hook_mode, 0o755);

        let state = dir.path().join("update.json");
        write_atomic(&state, b"{}", Some(0o600)).expect("write 0600");
        let state_mode = fs::metadata(&state)
            .expect("stat state")
            .permissions()
            .mode()
            & 0o7777;
        assert_eq!(state_mode, 0o600);

        let group_writable = dir.path().join("shared.conf");
        write_atomic(&group_writable, b"x", Some(0o664)).expect("write 0664");
        let group_mode = fs::metadata(&group_writable)
            .expect("stat shared")
            .permissions()
            .mode()
            & 0o7777;
        assert_eq!(group_mode, 0o664);
    }

    #[test]
    fn write_atomic_cleans_up_temp_file_on_failed_rename() {
        let dir = TempDir::new().expect("temp dir");
        let target_dir = dir.path().join("occupied");
        fs::create_dir(&target_dir).expect("create target dir");
        fs::write(target_dir.join("keep"), b"x").expect("populate target dir");

        let err = write_atomic(&target_dir, b"new data", None)
            .expect_err("renaming a file over a non-empty directory must fail");

        assert!(
            err.raw_os_error().is_some(),
            "expected OS rename error, got: {err}"
        );
        assert_eq!(
            dir_entries(dir.path()),
            vec![target_dir],
            "failed rename must not leave a temp file in the parent directory"
        );
    }

    #[test]
    fn write_atomic_rejects_path_without_parent() {
        let err = write_atomic(Path::new("/"), b"data", None)
            .expect_err("root path has no parent directory");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn temporary_path_generates_distinct_paths_in_same_process() {
        let dir = TempDir::new().expect("temp dir");
        let path_a = temporary_path(dir.path());
        let path_b = temporary_path(dir.path());

        assert_ne!(path_a, path_b);
        assert_eq!(path_a.parent(), Some(dir.path()));
        assert_eq!(path_b.parent(), Some(dir.path()));

        let name_a = path_a
            .file_name()
            .and_then(|n| n.to_str())
            .expect("utf-8 temp name");
        let name_b = path_b
            .file_name()
            .and_then(|n| n.to_str())
            .expect("utf-8 temp name");
        let pid_prefix = format!(".aegis-{}-", std::process::id());
        assert!(name_a.starts_with(&pid_prefix), "got {name_a}");
        assert!(name_a.ends_with(".tmp"), "got {name_a}");
        assert!(name_b.starts_with(&pid_prefix), "got {name_b}");
        assert!(name_b.ends_with(".tmp"), "got {name_b}");

        let seq_a = name_a
            .strip_suffix(".tmp")
            .and_then(|s| s.rsplit_once('-'))
            .map(|(_, seq)| seq);
        let seq_b = name_b
            .strip_suffix(".tmp")
            .and_then(|s| s.rsplit_once('-'))
            .map(|(_, seq)| seq);
        assert_ne!(seq_a, seq_b, "sequence numbers must advance across calls");
    }
}
