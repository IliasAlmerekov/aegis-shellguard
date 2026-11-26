use std::path::Path;

use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadSource {
    pub source: String,
    pub byte_len: u64,
    pub source_hash: String,
    pub source_byte_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceReadError {
    NotFound,
    NotRegularFile,
    PermissionDenied,
    TooLarge { size: u64, limit: u64 },
    InvalidUtf8,
    Io(String),
}

const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

pub async fn read_script_file(
    path: &Path,
    limit_bytes: u64,
) -> Result<ReadSource, SourceReadError> {
    let pre_metadata = tokio::fs::symlink_metadata(path)
        .await
        .map_err(map_open_error)?;
    if !pre_metadata.is_file() {
        return Err(SourceReadError::NotRegularFile);
    }

    let file = tokio::fs::File::open(path).await.map_err(map_open_error)?;
    let post_metadata = file.metadata().await.map_err(map_open_error)?;
    if !post_metadata.is_file() {
        return Err(SourceReadError::NotRegularFile);
    }

    let size = post_metadata.len();
    if size > limit_bytes {
        return Err(SourceReadError::TooLarge {
            size,
            limit: limit_bytes,
        });
    }

    let mut bytes = Vec::with_capacity(size.min(limit_bytes) as usize + 1);
    file.take(limit_bytes + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|err| SourceReadError::Io(err.to_string()))?;
    if bytes.len() as u64 > limit_bytes {
        return Err(SourceReadError::TooLarge {
            size: bytes.len() as u64,
            limit: limit_bytes,
        });
    }

    let byte_len = bytes.len() as u64;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let source_hash = super::sha256_hex(&hasher.finalize());

    let source_byte_offset = usize::from(bytes.starts_with(&UTF8_BOM)) * UTF8_BOM.len();
    let content = bytes.strip_prefix(&UTF8_BOM).unwrap_or(bytes.as_slice());
    let source = String::from_utf8(content.to_vec()).map_err(|_| SourceReadError::InvalidUtf8)?;

    Ok(ReadSource {
        source,
        byte_len,
        source_hash,
        source_byte_offset,
    })
}

fn map_open_error(err: std::io::Error) -> SourceReadError {
    match err.kind() {
        std::io::ErrorKind::NotFound => SourceReadError::NotFound,
        std::io::ErrorKind::PermissionDenied => SourceReadError::PermissionDenied,
        _ => SourceReadError::Io(err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn regular_file_under_limit_is_read_and_hashed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("script.py");
        std::fs::write(&path, "print(1)\n").unwrap();

        let result = read_script_file(&path, 1024).await.unwrap();

        assert_eq!(result.source, "print(1)\n");
        assert_eq!(result.byte_len, 9);
        assert_eq!(
            result.source_hash,
            "cc42155088fca5730758db72b2a5bca33112a941dfaa2d43098ec422ce4ea213"
        );
    }

    #[tokio::test]
    async fn missing_file_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist.py");

        assert_eq!(
            read_script_file(&path, 1024).await.unwrap_err(),
            SourceReadError::NotFound
        );
    }

    #[tokio::test]
    async fn directory_is_rejected_as_not_a_regular_file() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(
            read_script_file(dir.path(), 1024).await.unwrap_err(),
            SourceReadError::NotRegularFile
        );
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn symlink_is_rejected_without_following() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("real.py");
        std::fs::write(&target, "print(1)\n").unwrap();
        let link = dir.path().join("link.py");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert_eq!(
            read_script_file(&link, 1024).await.unwrap_err(),
            SourceReadError::NotRegularFile
        );
    }

    #[tokio::test]
    async fn oversized_file_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.py");
        std::fs::write(&path, "x".repeat(2048)).unwrap();

        assert_eq!(
            read_script_file(&path, 1024).await.unwrap_err(),
            SourceReadError::TooLarge {
                size: 2048,
                limit: 1024,
            }
        );
    }

    #[tokio::test]
    async fn invalid_utf8_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.py");
        std::fs::write(&path, [0xFF, 0xFE, 0x00]).unwrap();

        assert_eq!(
            read_script_file(&path, 1024).await.unwrap_err(),
            SourceReadError::InvalidUtf8
        );
    }

    #[tokio::test]
    async fn utf8_bom_is_stripped_from_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bom.py");
        let mut bytes = UTF8_BOM.to_vec();
        bytes.extend_from_slice(b"print(1)\n");
        std::fs::write(&path, &bytes).unwrap();

        let result = read_script_file(&path, 1024).await.unwrap();

        assert_eq!(result.source, "print(1)\n");
        assert_eq!(result.byte_len, bytes.len() as u64);
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn permission_denied_file_is_rejected() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret.py");
        std::fs::write(&path, "print(1)\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();

        let result = read_script_file(&path, 1024).await;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        if let Err(err) = result {
            assert_eq!(err, SourceReadError::PermissionDenied);
        }
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn unix_socket_is_rejected_as_not_a_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sock");
        let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();

        assert_eq!(
            read_script_file(&path, 1024).await.unwrap_err(),
            SourceReadError::NotRegularFile
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[cfg(unix)]
    async fn concurrent_symlink_swap_race_never_panics_or_corrupts_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("script.py");
        let other = dir.path().join("other.py");
        let content_a = "print('a')\n";
        let content_b = "print('bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb')\n";
        std::fs::write(&other, content_b).unwrap();

        const MAX_ATTEMPTS: u32 = 5;
        let mut observed_ok_total = 0;
        for _attempt in 0..MAX_ATTEMPTS {
            observed_ok_total +=
                run_symlink_swap_race_round(&path, &other, content_a, content_b).await;
            if observed_ok_total > 0 {
                break;
            }
        }

        assert!(
            observed_ok_total > 0,
            "expected at least some reads to succeed across {MAX_ATTEMPTS} race-window attempts"
        );
    }

    #[cfg(unix)]
    async fn run_symlink_swap_race_round(
        path: &std::path::Path,
        other: &std::path::Path,
        content_a: &str,
        content_b: &str,
    ) -> u32 {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let swap_stop = std::sync::Arc::clone(&stop);
        let swap_path = path.to_path_buf();
        let swap_other = other.to_path_buf();
        let swap_content_a = content_a.to_string();
        let swapper = std::thread::spawn(move || {
            let staging = swap_path.with_extension("staging");
            let mut toggle = false;
            while !swap_stop.load(std::sync::atomic::Ordering::Relaxed) {
                if toggle {
                    let _ = std::fs::remove_file(&staging);
                    let _ = std::os::unix::fs::symlink(&swap_other, &staging);
                } else {
                    let _ = std::fs::write(&staging, &swap_content_a);
                }
                let _ = std::fs::rename(&staging, &swap_path);
                toggle = !toggle;
            }
        });

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(750);
        let mut observed_ok = 0;
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(
                std::time::Duration::from_secs(1),
                read_script_file(path, 1024),
            )
            .await
            {
                Ok(Ok(result)) => {
                    assert!(
                        result.source == content_a || result.source == content_b,
                        "a successful read must return one of the two known-good contents, \
                         never a corrupted mix: got {:?}",
                        result.source
                    );
                    observed_ok += 1;
                }
                Ok(Err(_)) => {}
                Err(_) => panic!("read_script_file hung under the concurrent-swap race"),
            }
        }

        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        swapper.join().unwrap();
        observed_ok
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn fifo_is_rejected_as_not_a_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fifo");
        let status = std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .expect("mkfifo must be available in the test environment");
        assert!(status.success());

        assert_eq!(
            read_script_file(&path, 1024).await.unwrap_err(),
            SourceReadError::NotRegularFile
        );
    }
}
