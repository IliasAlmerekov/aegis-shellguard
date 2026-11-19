use crate::SnapshotError;
use std::time::Duration;

const EXECUTABLE_BUSY_ERRNO: i32 = 26;

pub(crate) fn is_executable_busy(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(EXECUTABLE_BUSY_ERRNO)
}

pub(crate) async fn retry<T, F, Fut>(
    mut launch: F,
    attempts: usize,
    bin: &str,
    family: &str,
    context: &str,
) -> Result<T, SnapshotError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = std::io::Result<T>>,
{
    let mut attempt = 0;
    loop {
        match launch().await {
            Ok(value) => return Ok(value),
            Err(error) if is_executable_busy(&error) && attempt < attempts => {
                attempt += 1;
                tracing::warn!(
                    bin,
                    context,
                    attempt,
                    "{} binary busy during command launch, retrying",
                    family
                );
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(error) => {
                return Err(SnapshotError::CommandLaunchFailed {
                    context: context.to_string(),
                    detail: error.to_string(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    #[tokio::test(flavor = "current_thread")]
    async fn persistent_busy_error_yields_to_runtime() {
        let task = tokio::spawn(async {});
        let dir = tempfile::tempdir().unwrap();
        let bin = crate::test_support::stub_bin(dir.path(), "busy", "exit 0");
        let _writer = std::fs::OpenOptions::new().write(true).open(&bin).unwrap();
        let mut command = tokio::process::Command::new(&bin);
        let result = super::retry(
            || std::future::ready(command.spawn()),
            1,
            "test",
            "test",
            "test context",
        )
        .await;
        assert!(task.is_finished());
        task.await.unwrap();
        assert!(
            matches!(result, Err(crate::SnapshotError::CommandLaunchFailed { context, detail }) if context == "test context" && detail == std::io::Error::from_raw_os_error(26).to_string())
        );
    }
}
