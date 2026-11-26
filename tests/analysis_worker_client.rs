#![cfg(feature = "language-analysis")]

use std::io;
use std::pin::Pin;
use std::process::Stdio;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use aegis::analysis::{RequestKind, TargetRequest, TargetResult, Worker, WorkerError, analyze};
use aegis_language::SourceLanguage;
use aegis_language::protocol::{self, MAX_SOURCE_BYTES, Response};
use aegis_types::DegradationReason;

fn req(id: u32, language: SourceLanguage, source: &[u8]) -> TargetRequest {
    TargetRequest {
        request_id: id,
        language,
        source: source.to_vec(),
        kind: RequestKind::Parse,
    }
}

fn resp_bytes(id: u32, resp: &Response) -> Vec<u8> {
    protocol::encode_response(id, resp).expect("test response encodes")
}

fn spawn_piped(cmd: &mut Command) -> (Child, ChildStdin, ChildStdout) {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("command must spawn");
    let stdin = child.stdin.take().expect("stdin piped");
    let stdout = child.stdout.take().expect("stdout piped");
    (child, stdin, stdout)
}

fn degrades_as_worker_failure(err: &WorkerError) -> bool {
    DegradationReason::from(err.clone()) == DegradationReason::WorkerFailure
}

#[tokio::test]
async fn analyze_round_trips_a_clean_parse_request_via_real_subprocess() {
    let mut worker = Worker::spawn(Some(env!("CARGO_BIN_EXE_aegis")))
        .await
        .expect("spawning the real worker must succeed");

    let results = worker
        .analyze(
            vec![req(1, SourceLanguage::Python, b"print(1)")],
            Duration::from_secs(2),
        )
        .await;
    assert_eq!(results.len(), 1);
    assert!(
        matches!(
            results[0],
            TargetResult::Responded(Response::Parsed { error_count: 0 })
        ),
        "clean Python source must round-trip: {results:?}"
    );

    let code = worker.shutdown().await.expect("shutdown must succeed");
    assert_eq!(code, Some(0), "a clean session must exit 0");
}

#[tokio::test]
async fn worker_analyze_reaps_the_child_so_exit_code_is_available_immediately() {
    let mut worker = Worker::spawn(Some(env!("CARGO_BIN_EXE_aegis")))
        .await
        .expect("spawning the real worker must succeed");

    let results = worker
        .analyze(
            vec![req(1, SourceLanguage::Python, b"print(1)")],
            Duration::from_secs(2),
        )
        .await;
    assert!(
        matches!(
            results[0],
            TargetResult::Responded(Response::Parsed { error_count: 0 })
        ),
        "clean source must round-trip: {results:?}"
    );
    assert_eq!(
        worker.exit_code(),
        Some(0),
        "Worker::analyze must close stdin and reap the child"
    );
}

fn printf_octal(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 4);
    for b in bytes {
        s.push_str(&format!("\\{b:03o}"));
    }
    s
}

#[tokio::test]
async fn worker_analyze_degrades_when_the_worker_responds_fully_then_exits_nonzero() {
    let frame = resp_bytes(1, &Response::Parsed { error_count: 0 });
    let script = format!(
        "cat >/dev/null 2>&1; printf '{}'; exit 3",
        printf_octal(&frame)
    );
    let mut cmd = Command::new("sh");
    cmd.args(["-c", &script]);
    let mut worker =
        Worker::spawn_command(cmd).expect("spawning the substitute worker must succeed");

    let results = worker
        .analyze(
            vec![req(1, SourceLanguage::Python, b"print(1)")],
            Duration::from_secs(2),
        )
        .await;
    assert_eq!(results.len(), 1);
    match &results[0] {
        TargetResult::Failed(WorkerError::NonZeroExit { code }) => {
            assert_eq!(*code, Some(3), "the substitute's exit-3 must be reported");
        }
        other => panic!(
            "expected Failed(NonZeroExit {{ code: Some(3) }}) — a non-zero exit after \
             responding must not be silent success, got {other:?}"
        ),
    }
}

#[tokio::test]
async fn worker_deadline_covers_a_blocked_request_write() {
    let mut cmd = Command::new("sh");
    cmd.args(["-c", "sleep 5"]);
    let mut worker =
        Worker::spawn_command(cmd).expect("spawning the substitute worker must succeed");
    let source = vec![b'x'; MAX_SOURCE_BYTES];
    let started = Instant::now();

    let results = worker
        .analyze(
            vec![req(1, SourceLanguage::Python, &source)],
            Duration::from_millis(50),
        )
        .await;

    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(matches!(
        results.as_slice(),
        [TargetResult::Failed(WorkerError::Timeout)]
    ));
}

#[tokio::test]
async fn worker_deadline_covers_reap_after_a_valid_response() {
    let frame = resp_bytes(1, &Response::Parsed { error_count: 0 });
    let script = format!(
        "cat >/dev/null 2>&1; printf '{}'; sleep 5",
        printf_octal(&frame)
    );
    let mut cmd = Command::new("sh");
    cmd.args(["-c", &script]);
    let mut worker =
        Worker::spawn_command(cmd).expect("spawning the substitute worker must succeed");
    let started = Instant::now();

    let results = worker
        .analyze(
            vec![req(1, SourceLanguage::Python, b"print(1)")],
            Duration::from_millis(50),
        )
        .await;

    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(matches!(
        results.as_slice(),
        [TargetResult::Failed(WorkerError::Timeout)]
    ));
}

#[tokio::test]
async fn analyze_reports_a_worker_failure_when_the_worker_exits_nonzero_without_responding() {
    let (_child, mut stdin, mut stdout) =
        spawn_piped(Command::new("sh").args(["-c", "cat >/dev/null 2>&1; exit 3"]));

    let results = analyze(
        &mut stdout,
        &mut stdin,
        vec![req(1, SourceLanguage::Python, b"print(1)")],
        Duration::from_secs(2),
    )
    .await;
    assert_eq!(results.len(), 1);
    match &results[0] {
        TargetResult::Failed(err) => assert!(
            degrades_as_worker_failure(err),
            "a non-zero worker exit must degrade as WorkerFailure: {err}"
        ),
        other => panic!("expected a failure, got {other:?}"),
    }
}

#[tokio::test]
async fn analyze_reports_protocol_noise_when_stdout_is_not_a_frame() {
    let (_child, mut stdin, mut stdout) = spawn_piped(
        Command::new("sh").args(["-c", "printf 'AAAAAAAAAAAAAAAA'; cat >/dev/null 2>&1"]),
    );

    let results = analyze(
        &mut stdout,
        &mut stdin,
        vec![req(1, SourceLanguage::Python, b"print(1)")],
        Duration::from_secs(2),
    )
    .await;
    assert_eq!(results.len(), 1);
    match &results[0] {
        TargetResult::Failed(WorkerError::ProtocolNoise(_)) => {}
        other => panic!("expected ProtocolNoise, got {other:?}"),
    }
}

#[tokio::test]
async fn analyze_times_out_when_the_worker_never_responds() {
    let (mut reader, _server_writer) = tokio::io::duplex(1024);
    let mut sink = tokio::io::sink();

    let results = analyze(
        &mut reader,
        &mut sink,
        vec![req(1, SourceLanguage::Python, b"print(1)")],
        Duration::from_millis(50),
    )
    .await;
    assert_eq!(results.len(), 1);
    match &results[0] {
        TargetResult::Failed(WorkerError::Timeout) => {}
        other => panic!("expected Timeout, got {other:?}"),
    }
}

#[tokio::test]
async fn analyze_detects_a_duplicate_response() {
    let (mut reader, mut server) = tokio::io::duplex(8192);
    server
        .write_all(&resp_bytes(1, &Response::Parsed { error_count: 0 }))
        .await
        .unwrap();
    server
        .write_all(&resp_bytes(1, &Response::Parsed { error_count: 0 }))
        .await
        .unwrap();
    server
        .write_all(&resp_bytes(2, &Response::Parsed { error_count: 0 }))
        .await
        .unwrap();
    drop(server);
    let mut sink = tokio::io::sink();

    let results = analyze(
        &mut reader,
        &mut sink,
        vec![
            req(1, SourceLanguage::Python, b"a = 1"),
            req(2, SourceLanguage::Python, b"b = 2"),
        ],
        Duration::from_secs(2),
    )
    .await;
    assert_eq!(results.len(), 2);
    assert!(
        matches!(
            results[0],
            TargetResult::Responded(Response::Parsed { error_count: 0 })
        ),
        "the first response is retained: {results:?}"
    );
    match &results[1] {
        TargetResult::Failed(WorkerError::DuplicateResponse { request_id: 1 }) => {}
        other => panic!("expected DuplicateResponse {{ request_id: 1 }}, got {other:?}"),
    }
}

#[tokio::test]
async fn analyze_detects_an_out_of_order_response() {
    let (mut reader, mut server) = tokio::io::duplex(8192);
    server
        .write_all(&resp_bytes(2, &Response::Parsed { error_count: 0 }))
        .await
        .unwrap();
    server
        .write_all(&resp_bytes(1, &Response::Parsed { error_count: 0 }))
        .await
        .unwrap();
    drop(server);
    let mut sink = tokio::io::sink();

    let results = analyze(
        &mut reader,
        &mut sink,
        vec![
            req(1, SourceLanguage::Python, b"a = 1"),
            req(2, SourceLanguage::Python, b"b = 2"),
        ],
        Duration::from_secs(2),
    )
    .await;
    assert_eq!(results.len(), 2);
    match &results[0] {
        TargetResult::Failed(WorkerError::OutOfOrder {
            expected: 1,
            got: 2,
        }) => {}
        other => panic!("expected OutOfOrder {{ expected: 1, got: 2 }}, got {other:?}"),
    }
    assert!(
        matches!(results[1], TargetResult::Failed(_)),
        "the not-yet-responded target is also failed: {results:?}"
    );
}

#[tokio::test]
async fn analyze_detects_a_response_for_an_id_that_was_never_sent() {
    let (mut reader, mut server) = tokio::io::duplex(8192);
    server
        .write_all(&resp_bytes(99, &Response::Parsed { error_count: 0 }))
        .await
        .unwrap();
    drop(server);
    let mut sink = tokio::io::sink();

    let results = analyze(
        &mut reader,
        &mut sink,
        vec![req(1, SourceLanguage::Python, b"a = 1")],
        Duration::from_secs(2),
    )
    .await;
    assert_eq!(results.len(), 1);
    match &results[0] {
        TargetResult::Failed(WorkerError::UnexpectedResponse { request_id: 99 }) => {}
        other => panic!("expected UnexpectedResponse {{ request_id: 99 }}, got {other:?}"),
    }
}

#[tokio::test]
async fn analyze_retains_prior_results_when_a_later_target_hits_eof() {
    let (mut reader, mut server) = tokio::io::duplex(8192);
    server
        .write_all(&resp_bytes(1, &Response::Parsed { error_count: 0 }))
        .await
        .unwrap();
    drop(server);
    let mut sink = tokio::io::sink();

    let results = analyze(
        &mut reader,
        &mut sink,
        vec![
            req(1, SourceLanguage::Python, b"a = 1"),
            req(2, SourceLanguage::Python, b"b = 2"),
            req(3, SourceLanguage::Python, b"c = 3"),
        ],
        Duration::from_secs(2),
    )
    .await;
    assert_eq!(results.len(), 3);
    assert!(
        matches!(
            results[0],
            TargetResult::Responded(Response::Parsed { error_count: 0 })
        ),
        "the first response is retained even though the session later failed: {results:?}"
    );
    assert!(
        matches!(results[1], TargetResult::Failed(WorkerError::Closed)),
        "the second target must fail Closed: {results:?}"
    );
    assert!(
        matches!(results[2], TargetResult::Failed(WorkerError::Closed)),
        "the third target must fail Closed: {results:?}"
    );
}

#[tokio::test]
async fn analyze_correlates_a_clean_multi_target_session_by_request_id() {
    let (mut reader, mut server) = tokio::io::duplex(8192);
    server
        .write_all(&resp_bytes(10, &Response::Parsed { error_count: 0 }))
        .await
        .unwrap();
    server
        .write_all(&resp_bytes(11, &Response::Parsed { error_count: 1 }))
        .await
        .unwrap();
    drop(server);
    let mut sink = tokio::io::sink();

    let results = analyze(
        &mut reader,
        &mut sink,
        vec![
            req(10, SourceLanguage::Python, b"a = 1"),
            req(11, SourceLanguage::Python, b"def f("),
        ],
        Duration::from_secs(2),
    )
    .await;
    assert_eq!(results.len(), 2);
    assert!(matches!(
        results[0],
        TargetResult::Responded(Response::Parsed { error_count: 0 })
    ));
    assert!(matches!(
        results[1],
        TargetResult::Responded(Response::Parsed { error_count: 1 })
    ));
}

struct FlushErrorWriter;

impl AsyncWrite for FlushErrorWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(Ok(buf.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "flush failed (reader gone)",
        )))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn analyze_surfaces_a_flush_failure_as_io_not_timeout() {
    let mut reader = tokio::io::empty();
    let mut writer = FlushErrorWriter;

    let results = analyze(
        &mut reader,
        &mut writer,
        vec![req(1, SourceLanguage::Python, b"print(1)")],
        Duration::from_secs(2),
    )
    .await;
    assert_eq!(results.len(), 1);
    match &results[0] {
        TargetResult::Failed(WorkerError::Io { kind, .. }) => {
            assert_eq!(*kind, io::ErrorKind::BrokenPipe);
        }
        other => panic!(
            "expected Failed(Io) from the flush error, got {other:?} \
             (Timeout would mean the flush error was dropped again)"
        ),
    }
}

#[tokio::test]
async fn analyze_degrades_an_oversized_request_without_panicking() {
    let mut reader = tokio::io::empty();
    let mut sink = tokio::io::sink();

    let results = analyze(
        &mut reader,
        &mut sink,
        vec![req(
            1,
            SourceLanguage::Python,
            &vec![b'x'; MAX_SOURCE_BYTES + 1],
        )],
        Duration::from_secs(2),
    )
    .await;
    assert_eq!(results.len(), 1);
    assert!(
        matches!(results[0], TargetResult::Failed(_)),
        "an oversized request must degrade, not panic: {results:?}"
    );
}
