use super::*;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::process::Command as StdCommand;
use tempfile::TempDir;

use crate::test_support::stub_bin;

fn plugin_with_user(temp_dir: &TempDir, user: &str) -> MysqlPlugin {
    MysqlPlugin::new(
        "app".to_string(),
        "localhost".to_string(),
        3306,
        user.to_string(),
        temp_dir.path().join("snaps"),
    )
}

#[cfg(unix)]
fn single_quote_for_shell(path: &Path) -> String {
    path.to_string_lossy().replace('\'', r"'\''")
}

#[cfg(unix)]
fn hold_file_open_for_writing(path: &Path) -> std::process::Child {
    let quoted_path = single_quote_for_shell(path);
    StdCommand::new("/bin/sh")
        .arg("-c")
        .arg(format!("exec 3>>'{quoted_path}'; sleep 0.3"))
        .spawn()
        .unwrap()
}

fn snapshot_id_for(database: &str, host: &str, port: u16, user: &str, dump_path: &Path) -> String {
    snapshot_id::join_v3(&[
        &snapshot_id::encode_bytes(database.as_bytes()),
        &snapshot_id::encode_bytes(host.as_bytes()),
        &port.to_string(),
        &snapshot_id::encode_bytes(user.as_bytes()),
        &snapshot_id::encode_path(dump_path),
    ])
}

#[tokio::test]
async fn rollback_rejects_artifact_outside_snapshot_store() {
    let temp_dir = TempDir::new().unwrap();
    let plugin = plugin_with_user(&temp_dir, "root");
    let outside_dump = temp_dir.path().join("outside.sql");
    fs::write(&outside_dump, b"outside").unwrap();
    let snapshot_id = snapshot_id_for("app", "localhost", 3306, "root", &outside_dump);

    let err = plugin.rollback(&snapshot_id).await.unwrap_err();

    assert!(matches!(
        err,
        SnapshotError::PathEscapesSnapshotStore {
            plugin: "mysql",
            ..
        }
    ));
}

#[tokio::test]
async fn delete_rejects_artifact_outside_snapshot_store() {
    let temp_dir = TempDir::new().unwrap();
    let plugin = plugin_with_user(&temp_dir, "root");
    let outside_dump = temp_dir.path().join("outside.sql");
    fs::write(&outside_dump, b"outside").unwrap();
    let snapshot_id = snapshot_id_for("app", "localhost", 3306, "root", &outside_dump);

    let err = plugin.delete(&snapshot_id).await.unwrap_err();

    assert!(matches!(
        err,
        SnapshotError::PathEscapesSnapshotStore {
            plugin: "mysql",
            ..
        }
    ));
    assert_eq!(fs::read(outside_dump).unwrap(), b"outside");
}

#[tokio::test]
async fn delete_rejects_parent_traversal_artifact_path() {
    let temp_dir = TempDir::new().unwrap();
    let plugin = plugin_with_user(&temp_dir, "root");
    fs::create_dir_all(&plugin.snapshots_dir).unwrap();
    let traversal_path = plugin.snapshots_dir.join("..").join("outside.sql");
    let snapshot_id = snapshot_id_for("app", "localhost", 3306, "root", &traversal_path);

    let err = plugin.delete(&snapshot_id).await.unwrap_err();

    assert!(matches!(
        err,
        SnapshotError::PathEscapesSnapshotStore {
            plugin: "mysql",
            ..
        }
    ));
}

#[tokio::test]
async fn is_not_applicable_when_database_empty() {
    let temp_dir = TempDir::new().unwrap();
    let plugin = MysqlPlugin::new(
        String::new(),
        "localhost".to_string(),
        3306,
        "root".to_string(),
        temp_dir.path().join("snaps"),
    );

    assert!(!plugin.is_applicable(temp_dir.path()).await);
}

#[test]
fn build_common_args_includes_host_port() {
    let temp_dir = TempDir::new().unwrap();
    let plugin = plugin_with_user(&temp_dir, "");

    assert_eq!(
        plugin.build_common_args(),
        vec!["--host=localhost".to_string(), "--port=3306".to_string(),]
    );
}

#[test]
fn build_common_args_includes_user_when_set() {
    let temp_dir = TempDir::new().unwrap();
    let plugin = plugin_with_user(&temp_dir, "app_user");
    let args = plugin.build_common_args();

    assert!(args.iter().any(|arg| arg == "--user=app_user"));
}

#[test]
fn build_common_args_omits_user_when_empty() {
    let temp_dir = TempDir::new().unwrap();
    let plugin = plugin_with_user(&temp_dir, "");

    assert_eq!(
        plugin.build_common_args(),
        vec!["--host=localhost".to_string(), "--port=3306".to_string(),]
    );
}

#[test]
fn reserve_dump_path_creates_unique_files_atomically() {
    let temp_dir = TempDir::new().unwrap();
    let snapshots_dir = temp_dir.path().join("snaps");
    fs::create_dir_all(&snapshots_dir).unwrap();
    let plugin = plugin_with_user(&temp_dir, "root");

    let first = plugin.reserve_dump_path(1_234).unwrap();
    let second = plugin.reserve_dump_path(1_234).unwrap();

    assert_ne!(first, second);
    assert!(first.exists());
    assert!(second.exists());
    assert_eq!(
        first.file_name().unwrap().to_string_lossy(),
        "mysql-app-1234.sql"
    );
    assert_eq!(
        second.file_name().unwrap().to_string_lossy(),
        "mysql-app-1234-1.sql"
    );
}

#[test]
fn reserve_dump_path_sanitizes_database_name_for_filenames() {
    let temp_dir = TempDir::new().unwrap();
    let snapshots_dir = temp_dir.path().join("snaps");
    fs::create_dir_all(&snapshots_dir).unwrap();
    let plugin = MysqlPlugin::new(
        "app/\tname:prod".to_string(),
        "localhost".to_string(),
        3306,
        "root".to_string(),
        snapshots_dir,
    );

    let dump_path = plugin.reserve_dump_path(1_234).unwrap();

    assert_eq!(
        dump_path.file_name().unwrap().to_string_lossy(),
        "mysql-app__name_prod-1234.sql"
    );
}

#[test]
fn snapshot_id_v3_round_trips_target_fields_and_dump_path() {
    let dump_path = Path::new("/tmp/mysql\tbackup.sql");
    let snapshot_id = snapshot_id_for(
        "app/\tname:prod",
        "db\tprimary",
        3_307,
        "root\tadmin",
        dump_path,
    );
    let parts = snapshot_id::split_v3(&snapshot_id, 5).unwrap();

    assert_eq!(
        snapshot_id::decode_string(parts[0], "x").unwrap(),
        "app/\tname:prod"
    );
    assert_eq!(
        snapshot_id::decode_string(parts[1], "x").unwrap(),
        "db\tprimary"
    );
    assert_eq!(parts[2], "3307");
    assert_eq!(
        snapshot_id::decode_string(parts[3], "x").unwrap(),
        "root\tadmin"
    );
    assert_eq!(
        snapshot_id::decode_path(parts[4], "dump path").unwrap(),
        dump_path
    );
}

#[tokio::test]
async fn rollback_errors_on_malformed_dump_path_encoding() {
    let temp_dir = TempDir::new().unwrap();
    let plugin = plugin_with_user(&temp_dir, "root");

    let err = plugin
        .rollback("v2\t617070\t6c6f63616c686f7374\t3306\t726f6f74\txyz")
        .await
        .unwrap_err();

    match err {
        SnapshotError::Snapshot(msg) => assert!(msg.contains("invalid dump path encoding")),
        other => panic!("expected malformed snapshot error, got {other:?}"),
    }
}

#[tokio::test]
async fn rollback_errors_when_dump_file_missing() {
    let temp_dir = TempDir::new().unwrap();
    let plugin = plugin_with_user(&temp_dir, "root");
    fs::create_dir_all(&plugin.snapshots_dir).unwrap();
    let missing_dump = plugin.snapshots_dir.join("missing.sql");
    let snapshot_id = snapshot_id_for("app", "localhost", 3_306, "root", &missing_dump);

    let err = plugin.rollback(&snapshot_id).await.unwrap_err();

    match err {
        SnapshotError::RollbackDumpNotFound { path } => {
            assert_eq!(path, missing_dump.to_string_lossy())
        }
        other => panic!("expected RollbackDumpNotFound, got {other:?}"),
    }
}

#[tokio::test]
async fn rollback_errors_on_malformed_id() {
    let temp_dir = TempDir::new().unwrap();
    let plugin = plugin_with_user(&temp_dir, "root");

    let err = plugin
        .rollback("not-a-valid-snapshot-id")
        .await
        .unwrap_err();

    match err {
        SnapshotError::Snapshot(msg) => assert!(msg.contains("malformed snapshot_id")),
        other => panic!("expected malformed snapshot error, got {other:?}"),
    }
}

#[tokio::test]
async fn rollback_errors_on_malformed_database_encoding() {
    let temp_dir = TempDir::new().unwrap();
    let plugin = plugin_with_user(&temp_dir, "root");

    let err = plugin
        .rollback("v2\txyz\t6c6f63616c686f7374\t3306\t726f6f74\t2f746d702f6578616d706c652e73716c")
        .await
        .unwrap_err();

    match err {
        SnapshotError::Snapshot(msg) => assert!(msg.contains("invalid database encoding")),
        other => panic!("expected malformed snapshot error, got {other:?}"),
    }
}

#[cfg(unix)]
#[tokio::test]
async fn snapshot_uses_mysqldump_and_creates_dump_file() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("mysqldump.args");
    let mysqldump = stub_bin(
        temp_dir.path(),
        "mysqldump",
        &format!(
            "log='{}'\n: > \"$log\"\nfor arg in \"$@\"; do\n  printf '%s\\n' \"$arg\" >> \"$log\"\ndone\nprintf 'dump-data'",
            log_path.display()
        ),
    );
    let mut plugin = plugin_with_user(&temp_dir, "root");
    plugin.mysqldump_bin = mysqldump.display().to_string();

    let snapshot_id = plugin
        .snapshot(temp_dir.path(), "dangerous command")
        .await
        .unwrap();
    let parts = snapshot_id::split_v3(&snapshot_id, 5).unwrap();
    let logged_args = fs::read_to_string(&log_path).unwrap();

    assert_eq!(snapshot_id::decode_string(parts[0], "x").unwrap(), "app");
    assert_eq!(
        snapshot_id::decode_string(parts[1], "x").unwrap(),
        "localhost"
    );
    assert_eq!(parts[2], "3306");
    assert_eq!(snapshot_id::decode_string(parts[3], "x").unwrap(), "root");
    assert!(logged_args.lines().any(|line| line == "--databases"));
    assert!(
        logged_args
            .lines()
            .any(|line| line == "--add-drop-database")
    );
    assert!(logged_args.lines().any(|line| line == "app"));
    let dump_path = snapshot_id::decode_path(parts[4], "dump path").unwrap();
    assert_eq!(fs::read_to_string(&dump_path).unwrap(), "dump-data");
    assert_eq!(
        fs::metadata(dump_path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(dump_path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[cfg(unix)]
#[tokio::test]
async fn snapshot_retries_when_mysqldump_binary_is_temporarily_busy() {
    let temp_dir = TempDir::new().unwrap();
    let mysqldump = stub_bin(temp_dir.path(), "mysqldump", "exit 0");
    let mut plugin = plugin_with_user(&temp_dir, "root");
    plugin.mysqldump_bin = mysqldump.display().to_string();

    let mut holder = hold_file_open_for_writing(&mysqldump);
    std::thread::sleep(std::time::Duration::from_millis(25));

    let snapshot_id = plugin
        .snapshot(temp_dir.path(), "dangerous command")
        .await
        .unwrap();

    let _ = holder.wait();
    assert!(snapshot_id.starts_with("v3:"));
}

#[cfg(unix)]
#[tokio::test]
async fn snapshot_returns_stderr_when_mysqldump_fails() {
    let temp_dir = TempDir::new().unwrap();
    let mysqldump = stub_bin(
        temp_dir.path(),
        "mysqldump",
        "printf 'mysqldump exploded' >&2\nexit 12",
    );
    let mut plugin = plugin_with_user(&temp_dir, "root");
    plugin.mysqldump_bin = mysqldump.display().to_string();

    let err = plugin
        .snapshot(temp_dir.path(), "dangerous command")
        .await
        .unwrap_err();

    match err {
        SnapshotError::Snapshot(msg) => {
            assert!(msg.contains("mysqldump failed"));
            assert!(msg.contains("mysqldump exploded"));
        }
        other => panic!("expected snapshot error, got {other:?}"),
    }
}

#[tokio::test]
async fn snapshot_removes_reserved_dump_when_mysqldump_spawn_fails() {
    let temp_dir = TempDir::new().unwrap();
    let snapshots_dir = temp_dir.path().join("snaps");
    let mut plugin = plugin_with_user(&temp_dir, "root");
    plugin.mysqldump_bin = temp_dir
        .path()
        .join("missing-mysqldump")
        .display()
        .to_string();

    let err = plugin
        .snapshot(temp_dir.path(), "dangerous command")
        .await
        .unwrap_err();

    match err {
        SnapshotError::Snapshot(msg) => assert!(msg.contains("failed to run mysqldump")),
        other => panic!("expected snapshot error, got {other:?}"),
    }
    assert!(snapshots_dir.exists());
    assert!(fs::read_dir(&snapshots_dir).unwrap().next().is_none());
}

#[cfg(unix)]
#[tokio::test]
async fn snapshot_drains_large_stderr_without_deadlocking() {
    let temp_dir = TempDir::new().unwrap();
    let mysqldump = stub_bin(
        temp_dir.path(),
        "mysqldump",
        "i=0\nwhile [ \"$i\" -lt 5000 ]; do\n  printf '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\\n' >&2\n  i=$((i + 1))\ndone\nprintf 'dump-data'",
    );
    let mut plugin = plugin_with_user(&temp_dir, "root");
    plugin.mysqldump_bin = mysqldump.display().to_string();

    let snapshot_id = plugin
        .snapshot(temp_dir.path(), "dangerous command")
        .await
        .unwrap();

    let parts = snapshot_id::split_v3(&snapshot_id, 5).unwrap();
    let dump_path = snapshot_id::decode_path(parts[4], "dump path").unwrap();
    assert_eq!(fs::read_to_string(dump_path).unwrap(), "dump-data");
}

#[cfg(unix)]
#[tokio::test]
async fn rollback_uses_mysql_with_expected_arguments_and_dump_on_stdin() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("mysql.args");
    let stdin_path = temp_dir.path().join("mysql.stdin");
    let dump_path = temp_dir.path().join("snaps").join("existing.sql");
    fs::create_dir_all(dump_path.parent().unwrap()).unwrap();
    fs::write(&dump_path, "dump-data").unwrap();
    let mysql = stub_bin(
        temp_dir.path(),
        "mysql",
        &format!(
            "log='{}'\nstdin_file='{}'\n: > \"$log\"\nfor arg in \"$@\"; do\n  printf '%s\\n' \"$arg\" >> \"$log\"\ndone\ncat > \"$stdin_file\"",
            log_path.display(),
            stdin_path.display()
        ),
    );
    let mut plugin = plugin_with_user(&temp_dir, "root");
    plugin.mysql_bin = mysql.display().to_string();
    let snapshot_id = snapshot_id_for("app", "localhost", 3_306, "root", &dump_path);

    plugin.rollback(&snapshot_id).await.unwrap();

    let logged_args = fs::read_to_string(&log_path).unwrap();
    assert!(logged_args.lines().any(|line| line == "--host=localhost"));
    assert!(logged_args.lines().any(|line| line == "--port=3306"));
    assert!(logged_args.lines().any(|line| line == "--user=root"));
    assert!(!logged_args.lines().any(|line| line == "app"));
    assert_eq!(fs::read_to_string(&stdin_path).unwrap(), "dump-data");
}

#[cfg(unix)]
#[tokio::test]
async fn rollback_retries_when_mysql_binary_is_temporarily_busy() {
    let temp_dir = TempDir::new().unwrap();
    let dump_path = temp_dir.path().join("snaps").join("existing.sql");
    fs::create_dir_all(dump_path.parent().unwrap()).unwrap();
    fs::write(&dump_path, "dump-data").unwrap();
    let mysql = stub_bin(temp_dir.path(), "mysql", "cat > /dev/null");
    let mut plugin = plugin_with_user(&temp_dir, "root");
    plugin.mysql_bin = mysql.display().to_string();
    let snapshot_id = snapshot_id_for("app", "localhost", 3_306, "root", &dump_path);

    let mut holder = hold_file_open_for_writing(&mysql);
    std::thread::sleep(std::time::Duration::from_millis(25));

    plugin.rollback(&snapshot_id).await.unwrap();

    let _ = holder.wait();
}

#[tokio::test]
async fn rollback_restores_legacy_artifact_inside_snapshot_store() {
    let temp_dir = TempDir::new().unwrap();
    let dump_path = temp_dir.path().join("snaps").join("legacy.sql");
    fs::create_dir_all(dump_path.parent().unwrap()).unwrap();
    fs::write(&dump_path, "dump-data").unwrap();
    let mysql = stub_bin(temp_dir.path(), "mysql", "cat > /dev/null");
    let mut plugin = plugin_with_user(&temp_dir, "root");
    plugin.mysql_bin = mysql.display().to_string();
    let snapshot_id = "617070\t6c65676163792e73716c".to_string();

    plugin.rollback(&snapshot_id).await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn rollback_uses_snapshot_time_target_and_dump_path_instead_of_current_config() {
    let temp_dir = TempDir::new().unwrap();
    let stdin_path = temp_dir.path().join("mysql.stdin");
    let restore_log_path = temp_dir.path().join("mysql.args");
    let mysqldump = stub_bin(temp_dir.path(), "mysqldump", "printf 'dump-data'");
    let mysql = stub_bin(
        temp_dir.path(),
        "mysql",
        &format!(
            "log='{}'\nstdin_file='{}'\n: > \"$log\"\nfor arg in \"$@\"; do\n  printf '%s\\n' \"$arg\" >> \"$log\"\ndone\ncat > \"$stdin_file\"",
            restore_log_path.display(),
            stdin_path.display()
        ),
    );

    let mut snapshot_plugin = MysqlPlugin::new(
        "app".to_string(),
        "snapshot-host".to_string(),
        3_307,
        "snapshot-user".to_string(),
        temp_dir.path().join("old-snaps"),
    );
    snapshot_plugin.mysqldump_bin = mysqldump.display().to_string();

    let snapshot_id = snapshot_plugin
        .snapshot(temp_dir.path(), "dangerous command")
        .await
        .unwrap();

    let mut rollback_plugin = MysqlPlugin::new(
        "app".to_string(),
        "drifted-host".to_string(),
        4_321,
        "drifted-user".to_string(),
        temp_dir.path().join("old-snaps"),
    );
    rollback_plugin.mysql_bin = mysql.display().to_string();

    rollback_plugin.rollback(&snapshot_id).await.unwrap();

    let logged_args = fs::read_to_string(&restore_log_path).unwrap();
    assert!(
        logged_args
            .lines()
            .any(|line| line == "--host=snapshot-host")
    );
    assert!(logged_args.lines().any(|line| line == "--port=3307"));
    assert!(
        logged_args
            .lines()
            .any(|line| line == "--user=snapshot-user")
    );
    assert!(
        !logged_args
            .lines()
            .any(|line| line == "--host=drifted-host")
    );
    assert!(!logged_args.lines().any(|line| line == "--port=4321"));
    assert!(
        !logged_args
            .lines()
            .any(|line| line == "--user=drifted-user")
    );
    assert_eq!(fs::read_to_string(&stdin_path).unwrap(), "dump-data");
}

#[cfg(unix)]
#[tokio::test]
async fn rollback_returns_stderr_when_mysql_fails() {
    let temp_dir = TempDir::new().unwrap();
    let dump_path = temp_dir.path().join("snaps").join("existing.sql");
    fs::create_dir_all(dump_path.parent().unwrap()).unwrap();
    fs::write(&dump_path, "dump-data").unwrap();
    let mysql = stub_bin(
        temp_dir.path(),
        "mysql",
        "cat > /dev/null\nprintf 'mysql exploded' >&2\nexit 23",
    );
    let mut plugin = plugin_with_user(&temp_dir, "root");
    plugin.mysql_bin = mysql.display().to_string();
    let snapshot_id = snapshot_id_for("app", "localhost", 3_306, "root", &dump_path);

    let err = plugin.rollback(&snapshot_id).await.unwrap_err();

    match err {
        SnapshotError::Snapshot(msg) => {
            assert!(msg.contains("mysql failed"));
            assert!(msg.contains("mysql exploded"));
        }
        other => panic!("expected snapshot error, got {other:?}"),
    }
}

#[cfg(unix)]
#[tokio::test]
async fn rollback_errors_when_mysql_stdin_streaming_fails() {
    let temp_dir = TempDir::new().unwrap();
    let dump_path = temp_dir.path().join("snaps").join("existing.sql");
    fs::create_dir_all(dump_path.parent().unwrap()).unwrap();
    fs::write(&dump_path, vec![b'x'; 512 * 1024]).unwrap();
    let mysql = stub_bin(temp_dir.path(), "mysql", "exit 0");
    let mut plugin = plugin_with_user(&temp_dir, "root");
    plugin.mysql_bin = mysql.display().to_string();
    let snapshot_id = snapshot_id_for("app", "localhost", 3_306, "root", &dump_path);

    let err = plugin.rollback(&snapshot_id).await.unwrap_err();

    match err {
        SnapshotError::Snapshot(msg) => assert!(msg.contains("failed to write mysql stdin")),
        other => panic!("expected snapshot error, got {other:?}"),
    }
}

#[cfg(unix)]
#[tokio::test]
async fn rollback_drains_large_stderr_without_deadlocking() {
    let temp_dir = TempDir::new().unwrap();
    let dump_path = temp_dir.path().join("snaps").join("existing.sql");
    fs::create_dir_all(dump_path.parent().unwrap()).unwrap();
    fs::write(&dump_path, "dump-data").unwrap();
    let mysql = stub_bin(
        temp_dir.path(),
        "mysql",
        "i=0\nwhile [ \"$i\" -lt 5000 ]; do\n  printf '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\\n' >&2\n  i=$((i + 1))\ndone\ncat > /dev/null",
    );
    let mut plugin = plugin_with_user(&temp_dir, "root");
    plugin.mysql_bin = mysql.display().to_string();
    let snapshot_id = snapshot_id_for("app", "localhost", 3_306, "root", &dump_path);

    plugin.rollback(&snapshot_id).await.unwrap();
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "current_thread")]
async fn spawn_with_busy_retry_yields_to_tokio_runtime_during_sleep() {
    use std::sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    };

    let temp_dir = TempDir::new().unwrap();
    // A stub binary that we will hold open for writing for the entire test,
    // so every call to spawn() returns ETXTBSY and the retry loop fires.
    let mysqldump = stub_bin(temp_dir.path(), "mysqldump", "exit 0");

    // Keep the file open for writing from this process.  The kernel returns
    // ETXTBSY from execve() as long as any process holds the file open for
    // writing, so this forces spawn_with_busy_retry to exhaust all retries.
    let _keep_open = std::fs::OpenOptions::new()
        .append(true)
        .open(&mysqldump)
        .unwrap();

    // Spawn a concurrent task that counts how many scheduling opportunities
    // it receives.  In a current_thread runtime:
    //   - std::thread::sleep blocks the only worker thread, so this task
    //     cannot run at all during each 25 ms retry delay.
    //   - tokio::time::sleep(...).await yields to the runtime, letting this
    //     task run between retries.
    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = Arc::clone(&counter);
    tokio::spawn(async move {
        loop {
            tokio::task::yield_now().await;
            counter_clone.fetch_add(1, Ordering::Relaxed);
        }
    });

    // Call the retry function directly so we reach the sleep on every
    // attempt.  The binary is permanently busy, so all BUSY_RETRY_ATTEMPTS
    // (12) are exhausted and the call returns Err — no subsequent async I/O
    // happens, so the only yield opportunities are inside the retry sleep.
    let mut command = tokio::process::Command::new(&mysqldump);
    let result = MysqlPlugin::spawn_with_busy_retry(&mut command, "test context").await;

    // The binary was always busy, so we expect an error after exhausting retries.
    assert!(
        result.is_err(),
        "expected error after exhausting busy retries, got success"
    );

    // If std::thread::sleep was used, the counter is 0: the retry delays
    // blocked the single Tokio thread, giving the spawned task no chance to
    // run.  When fixed with tokio::time::sleep(...).await, the counter is
    // > 0 because each sleep yields to the executor.
    assert!(
        counter.load(Ordering::Relaxed) > 0,
        "counter stayed at 0: std::thread::sleep is blocking the Tokio \
         runtime during retry delays; replace with \
         tokio::time::sleep(...).await"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn snapshot_generates_distinct_ids_for_back_to_back_calls() {
    let temp_dir = TempDir::new().unwrap();
    let mysqldump = stub_bin(temp_dir.path(), "mysqldump", "printf 'dump-data'");
    let mut plugin = plugin_with_user(&temp_dir, "root");
    plugin.mysqldump_bin = mysqldump.display().to_string();

    let first_id = plugin
        .snapshot(temp_dir.path(), "dangerous command")
        .await
        .unwrap();
    let second_id = plugin
        .snapshot(temp_dir.path(), "dangerous command")
        .await
        .unwrap();
    let first_parts = snapshot_id::split_v3(&first_id, 5).unwrap();
    let second_parts = snapshot_id::split_v3(&second_id, 5).unwrap();
    let first_dump = snapshot_id::decode_path(first_parts[4], "dump path").unwrap();
    let second_dump = snapshot_id::decode_path(second_parts[4], "dump path").unwrap();

    assert_ne!(first_id, second_id);
    assert_ne!(first_dump, second_dump);
    assert!(first_dump.exists());
    assert!(second_dump.exists());
}

mod liveness_tests;

mod readiness_tests;

#[test]
fn build_snapshot_id_writes_v3_layout() {
    let temp_dir = TempDir::new().unwrap();
    let plugin = plugin_with_user(&temp_dir, "root");

    assert_eq!(
        plugin.build_snapshot_id(Path::new("/tmp/d.sql")),
        "v3:617070:6c6f63616c686f7374:3306:726f6f74:2f746d702f642e73716c"
    );
}

#[test]
fn parse_snapshot_id_reads_v3_and_v2_to_the_same_target() {
    let temp_dir = TempDir::new().unwrap();
    let plugin = plugin_with_user(&temp_dir, "root");

    for id in [
        "v3:617070:6c6f63616c686f7374:3306:726f6f74:2f746d702f642e73716c",
        "v2\t617070\t6c6f63616c686f7374\t3306\t726f6f74\t2f746d702f642e73716c",
    ] {
        let target = plugin.parse_snapshot_id(id).unwrap();
        assert_eq!(target.host, "localhost", "{id}");
        assert_eq!(target.port, 3306, "{id}");
        assert_eq!(target.user, "root", "{id}");
        assert_eq!(target.dump_path, Path::new("/tmp/d.sql"), "{id}");
    }
}

#[test]
fn parse_snapshot_id_rejects_v3_with_wrong_component_count() {
    let temp_dir = TempDir::new().unwrap();
    let plugin = plugin_with_user(&temp_dir, "root");

    assert!(
        plugin
            .parse_snapshot_id("v3:617070:6c6f63616c686f7374")
            .is_err()
    );
}

#[test]
fn parse_snapshot_id_reads_unversioned_legacy_id_against_the_snapshot_store() {
    let temp_dir = TempDir::new().unwrap();
    let plugin = plugin_with_user(&temp_dir, "root");

    let target = plugin
        .parse_snapshot_id("617070\t6c65676163792e73716c")
        .unwrap();

    assert_eq!(
        target.dump_path,
        temp_dir.path().join("snaps").join("legacy.sql")
    );
    assert_eq!(target.user, "root");
}
