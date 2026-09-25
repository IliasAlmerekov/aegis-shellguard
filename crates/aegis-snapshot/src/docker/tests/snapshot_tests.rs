use super::*;

#[tokio::test]
async fn is_applicable_no_docker_cli() {
    let p = DockerPlugin {
        docker_bin: "/nonexistent/bin/docker".to_string(),
        scope: DockerScope::default(),
    };
    assert!(!p.is_applicable(Path::new("/")).await);
}

#[tokio::test]
async fn is_applicable_no_running_containers() {
    let dir = TempDir::new().unwrap();
    write_mock_docker(
        dir.path(),
        r#"case "$1" in
  ps) exit 0 ;;
  *) exit 1 ;;
esac"#,
    );
    assert!(
        !plugin(&dir.path().join("docker"))
            .is_applicable(Path::new("/"))
            .await
    );
}

#[tokio::test]
async fn is_applicable_with_running_containers() {
    let dir = TempDir::new().unwrap();
    write_mock_docker(
        dir.path(),
        r#"case "$1" in
  ps) printf "abc123\n"; exit 0 ;;
  *) exit 1 ;;
esac"#,
    );
    assert!(
        plugin(&dir.path().join("docker"))
            .is_applicable(Path::new("/"))
            .await
    );
}

#[tokio::test]
async fn is_applicable_docker_not_running() {
    let dir = TempDir::new().unwrap();
    write_mock_docker(
        dir.path(),
        r#"case "$1" in
  ps) echo "Cannot connect to the Docker daemon" >&2; exit 1 ;;
  *) exit 1 ;;
esac"#,
    );
    assert!(
        !plugin(&dir.path().join("docker"))
            .is_applicable(Path::new("/"))
            .await
    );
}

// ── snapshot ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn snapshot_returns_sentinel_when_no_containers() {
    let dir = TempDir::new().unwrap();
    write_mock_docker(
        dir.path(),
        r#"case "$1" in
  ps) exit 0 ;;
  *) exit 1 ;;
esac"#,
    );
    let id = plugin(&dir.path().join("docker"))
        .snapshot(Path::new("/"), "rm -rf /")
        .await
        .unwrap();
    assert_eq!(id, NO_CONTAINERS);
}

#[tokio::test]
async fn snapshot_commits_each_running_container() {
    let dir = TempDir::new().unwrap();
    let inspect_json = MINIMAL_INSPECT;
    write_mock_docker(
        dir.path(),
        &format!(
            r#"case "$1" in
  ps)      printf "abc123\ndef456\n"; exit 0 ;;
  inspect) printf '{inspect_json}'; exit 0 ;;
  commit)  printf "sha256:mockhash\n"; exit 0 ;;
  *)       exit 1 ;;
esac"#
        ),
    );
    let id = plugin(&dir.path().join("docker"))
        .snapshot(Path::new("/"), "docker rm -f web")
        .await
        .unwrap();

    assert_eq!(id.lines().count(), 2, "one JSON record per container");
    assert!(id.contains("abc123"), "snapshot_id must reference abc123");
    assert!(id.contains("def456"), "snapshot_id must reference def456");
    assert!(id.contains("aegis-snap-"), "must use aegis-snap- prefix");

    // Each line must be valid JSON with the expected fields.
    for line in id.lines() {
        let rec: ContainerRecord = serde_json::from_str(line)
            .expect("each snapshot_id line must be a valid ContainerRecord");
        assert!(rec.image.starts_with("aegis-snap-"));
    }
}

#[tokio::test]
async fn snapshot_captures_container_metadata_from_inspect() {
    let dir = TempDir::new().unwrap();
    let inspect_json = RICH_INSPECT;
    write_mock_docker(
        dir.path(),
        &format!(
            r#"case "$1" in
  ps)      printf "abc123\n"; exit 0 ;;
  inspect) printf '{inspect_json}'; exit 0 ;;
  commit)  printf "sha256:mockhash\n"; exit 0 ;;
  *)       exit 1 ;;
esac"#
        ),
    );
    let id = plugin(&dir.path().join("docker"))
        .snapshot(Path::new("/"), "docker stop web")
        .await
        .unwrap();

    let rec: ContainerRecord = serde_json::from_str(id.trim()).unwrap();
    assert_eq!(rec.config.name, "web");
    assert_eq!(rec.config.network_mode, "my-net");
    assert_eq!(rec.config.restart_policy, "always");
    assert_eq!(rec.config.binds, vec!["/data:/app/data"]);
    assert!(
        rec.config.port_bindings.iter().any(|p| p.contains("8080")),
        "port binding must reference host port 8080"
    );
    assert_eq!(rec.config.labels.get("app"), Some(&"frontend".to_string()));
}

#[tokio::test]
async fn snapshot_fails_when_inspect_returns_error() {
    let dir = TempDir::new().unwrap();
    write_mock_docker(
        dir.path(),
        r#"case "$1" in
  ps)      printf "abc123\n"; exit 0 ;;
  inspect) printf "Error: no such container\n" >&2; exit 1 ;;
  *)       exit 1 ;;
esac"#,
    );
    let result = plugin(&dir.path().join("docker"))
        .snapshot(Path::new("/"), "rm -rf /")
        .await;
    assert!(result.is_err(), "snapshot must propagate inspect failure");
}

#[tokio::test]
async fn snapshot_fails_when_commit_returns_error() {
    let dir = TempDir::new().unwrap();
    let inspect_json = MINIMAL_INSPECT;
    write_mock_docker(
        dir.path(),
        &format!(
            r#"case "$1" in
  ps)      printf "abc123\n"; exit 0 ;;
  inspect) printf '{inspect_json}'; exit 0 ;;
  commit)  printf "Error: permission denied\n" >&2; exit 1 ;;
  *)       exit 1 ;;
esac"#
        ),
    );
    let result = plugin(&dir.path().join("docker"))
        .snapshot(Path::new("/"), "rm -rf /")
        .await;
    assert!(result.is_err(), "snapshot must propagate commit failure");
}

// ── rollback ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn rollback_noop_for_no_containers_sentinel() {
    // Must succeed without touching any docker binary.
    DockerPlugin::default()
        .rollback(NO_CONTAINERS)
        .await
        .unwrap();
}

#[tokio::test]
async fn rollback_stops_removes_then_recreates_container() {
    let dir = TempDir::new().unwrap();
    let log = dir.path().join("calls.log");
    let log_path = log.to_string_lossy().into_owned();

    write_mock_docker(
        dir.path(),
        &format!(
            r#"printf "%s\n" "$*" >> {log_path}
case "$1" in
  stop) exit 0 ;;
  rm)   exit 0 ;;
  run)  printf "newcontainer\n"; exit 0 ;;
  *)    exit 1 ;;
esac"#
        ),
    );

    let snapshot_id = minimal_record("abc123", "aegis-snap-abc123-1700000000");
    plugin(&dir.path().join("docker"))
        .rollback(&snapshot_id)
        .await
        .unwrap();

    let calls = fs::read_to_string(&log).unwrap();
    assert!(calls.contains("stop abc123"), "must call docker stop");
    assert!(
        calls.contains("rm abc123"),
        "must call docker rm to free the name"
    );
    assert!(
        calls.contains("aegis-snap-abc123-1700000000"),
        "must recreate from snapshot image"
    );
}

#[tokio::test]
async fn rollback_uses_captured_name_ports_and_network() {
    let dir = TempDir::new().unwrap();
    let log = dir.path().join("calls.log");
    let log_path = log.to_string_lossy().into_owned();

    write_mock_docker(
        dir.path(),
        &format!(
            r#"printf "%s\n" "$*" >> {log_path}
case "$1" in
  stop) exit 0 ;;
  rm)   exit 0 ;;
  run)  printf "newcontainer\n"; exit 0 ;;
  *)    exit 1 ;;
esac"#
        ),
    );

    let record = ContainerRecord {
        container_id: "abc123".to_string(),
        image: "aegis-snap-abc123-1700000000".to_string(),
        config: ContainerConfig {
            name: "web".to_string(),
            binds: vec!["/data:/app/data".to_string()],
            port_bindings: vec!["8080:80/tcp".to_string()],
            labels: HashMap::new(),
            network_mode: "my-net".to_string(),
            restart_policy: "always".to_string(),
        },
    };
    let snapshot_id = serde_json::to_string(&record).unwrap();

    plugin(&dir.path().join("docker"))
        .rollback(&snapshot_id)
        .await
        .unwrap();

    let calls = fs::read_to_string(&log).unwrap();
    assert!(calls.contains("--name web"), "must restore container name");
    assert!(
        calls.contains("-p 8080:80/tcp"),
        "must restore port binding"
    );
    assert!(
        calls.contains("-v /data:/app/data"),
        "must restore bind mount"
    );
    assert!(calls.contains("--network my-net"), "must restore network");
    assert!(
        calls.contains("--restart always"),
        "must restore restart policy"
    );
}

#[tokio::test]
async fn rollback_continues_when_stop_fails() {
    let dir = TempDir::new().unwrap();
    write_mock_docker(
        dir.path(),
        r#"case "$1" in
  stop) printf "No such container\n" >&2; exit 1 ;;
  rm)   exit 0 ;;
  run)  printf "newcontainer\n"; exit 0 ;;
  *)    exit 1 ;;
esac"#,
    );
    let snapshot_id = minimal_record("abc123", "aegis-snap-abc123-1700000000");
    // Must succeed despite stop failure.
    plugin(&dir.path().join("docker"))
        .rollback(&snapshot_id)
        .await
        .unwrap();
}

// ── async-safety regression tests ─────────────────────────────────────────

/// Proves `is_applicable` does not block the Tokio thread, via a causal
/// dependency instead of relative sleep durations (the prior version raced a
/// 50ms mock-`ps` sleep against a 10ms background sleep, and flaked under
/// load — issue #436).
///
/// The mock `ps` will not answer until a marker file exists on disk, bounded
/// to 5s of 10ms polls so a genuine regression fails fast rather than
/// hanging. The background task writes that marker only *after* incrementing
/// the counter, so by the time `ps` (and therefore `is_applicable`) can
/// return, the counter increment has already happened-before it on the
/// filesystem — no timing assumption needed for the green case.
///
/// With a blocking `is_applicable` on a `current_thread` runtime, the single
/// Tokio thread never yields to run the background task, so the marker is
/// never written, `ps` exhausts its poll budget, and `counter_after` stays 0.
#[tokio::test(flavor = "current_thread")]
async fn is_applicable_does_not_block_tokio_runtime_v2() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let dir = TempDir::new().unwrap();
    let marker = dir.path().join("bg-task-ran");
    let marker_path = marker.to_string_lossy().into_owned();
    write_mock_docker(
        dir.path(),
        &format!(
            r#"case "$1" in
  ps)
    i=0
    while [ ! -f '{marker_path}' ] && [ "$i" -lt 500 ]; do
      sleep 0.01
      i=$((i + 1))
    done
    printf "abc123\n"; exit 0 ;;
  *) exit 1 ;;
esac"#
        ),
    );

    let p = plugin(&dir.path().join("docker"));
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_bg = Arc::clone(&counter);

    let bg = tokio::task::spawn(async move {
        // Order matters: the counter is bumped before the marker is written,
        // so `ps` seeing the marker implies the counter is already updated.
        counter_bg.fetch_add(1, Ordering::SeqCst);
        std::fs::write(&marker, b"").unwrap();
    });

    // Async call — yields the Tokio thread while waiting for docker ps,
    // which only exits once the background task has run.
    let _ = p.is_applicable(Path::new("/")).await;

    // Read counter *before* awaiting bg. With blocking is_applicable the bg task
    // was never polled while is_applicable ran, so counter is still 0 here.
    let counter_after = counter.load(Ordering::SeqCst);

    bg.await.unwrap();

    // This assertion FAILS with a blocking implementation: counter_after
    // stays 0 because ps never saw the marker and timed out after 5s.
    assert_eq!(
        counter_after, 1,
        "is_applicable blocked the Tokio thread — background task could not progress \
         (counter={counter_after}, expected 1). Fix: make is_applicable async and use \
         tokio::process::Command."
    );
}

/// Verifies that `sleep_docker_busy_retry_delay` yields the current-thread runtime
/// so that other tasks can make progress during the delay.
///
/// The retry delay must use `tokio::time::sleep`, which is cheaper than
/// dispatching a blocking sleep to a helper thread and still yields the
/// runtime while the delay is pending.
///
/// This is a behavioral contract test: it asserts the function MUST yield the runtime
/// and will catch any regression that changes it to a blocking call (e.g., replacing
/// the body with a bare `thread::sleep` call).
#[tokio::test(flavor = "current_thread")]
async fn sleep_docker_busy_retry_delay_yields_to_runtime() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let flag = Arc::new(AtomicBool::new(false));
    let flag_bg = Arc::clone(&flag);

    // Background task sets the flag after 5ms.
    let bg = tokio::task::spawn(async move {
        tokio::time::sleep(Duration::from_millis(5)).await;
        flag_bg.store(true, Ordering::SeqCst);
    });

    // sleep_docker_busy_retry_delay sleeps for DOCKER_BUSY_RETRY_DELAY_MS (25ms).
    // A properly yielding implementation allows the bg task (5ms) to
    // complete before this returns.
    sleep_docker_busy_retry_delay().await;

    let flag_value = flag.load(Ordering::SeqCst);

    bg.await.unwrap();

    assert!(
        flag_value,
        "sleep_docker_busy_retry_delay did not yield to the runtime — \
         background task flag was not set (expected true, got false). \
         This would be a regression; the function must not block the Tokio thread."
    );
}
