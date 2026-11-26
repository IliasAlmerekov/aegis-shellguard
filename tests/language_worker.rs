#![cfg(feature = "language-analysis")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail loudly"
)]

mod support;

use std::io::{Read, Write};
use std::process::{Command, Stdio};

use aegis_language::SourceLanguage;
use aegis_language::protocol::{self, DecodedFrame, Request, Response};
use support::aegis_bin;

fn spawn_worker_with_args(leading_args: &[&str]) -> std::process::Child {
    Command::new(aegis_bin())
        .args(leading_args)
        .arg("--internal-language-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the aegis binary must be buildable and spawnable")
}

fn spawn_worker() -> std::process::Child {
    spawn_worker_with_args(&[])
}

fn encode_requests(requests: &[(u32, Request)]) -> Vec<u8> {
    let mut buf = Vec::new();
    for (id, req) in requests {
        buf.extend_from_slice(&protocol::encode_request(*id, req).expect("test source encodes"));
    }
    buf
}

fn decode_all_responses(buf: &[u8]) -> Vec<DecodedFrame<Response>> {
    let mut out = Vec::new();
    let mut rest = buf;
    while !rest.is_empty() {
        let frame = protocol::decode_response(rest)
            .expect("worker stdout must be a sequence of well-formed response frames")
            .expect("a non-empty buffer must contain a complete frame");
        let consumed = frame.consumed;
        out.push(frame);
        rest = &rest[consumed..];
    }
    out
}

fn parse_request(language: SourceLanguage, source: &[u8]) -> Request {
    Request::Parse {
        language,
        source: source.to_vec(),
    }
}

fn run_session(input: &[u8]) -> (Vec<u8>, std::process::ExitStatus) {
    let mut child = spawn_worker();
    {
        let mut stdin = child.stdin.take().expect("stdin must be piped");
        stdin
            .write_all(input)
            .expect("writing requests to the worker must succeed");
    }
    let mut stdout = child.stdout.take().expect("stdout must be piped");
    let mut out = Vec::new();
    stdout
        .read_to_end(&mut out)
        .expect("reading worker stdout must succeed");
    let status = child.wait().expect("the worker must terminate");
    (out, status)
}

#[test]
fn worker_subprocess_round_trips_a_clean_parse_request() {
    let input = encode_requests(&[(1, parse_request(SourceLanguage::Python, b"print(1)"))]);
    let (out, status) = run_session(&input);

    assert!(status.success(), "a clean session must exit 0");
    let responses = decode_all_responses(&out);
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0].request_id, 1);
    assert_eq!(
        responses[0].message,
        Response::Parsed { error_count: 0 },
        "clean Python source must round-trip through the real subprocess"
    );
}

#[test]
fn worker_subprocess_writes_only_frame_bytes_to_stdout() {
    let input = encode_requests(&[
        (1, parse_request(SourceLanguage::Python, b"x = 1")),
        (2, parse_request(SourceLanguage::Bash, b"echo hi")),
    ]);
    let (out, status) = run_session(&input);

    assert!(status.success());
    let responses = decode_all_responses(&out);
    assert_eq!(
        responses.iter().map(|f| f.request_id).collect::<Vec<_>>(),
        vec![1, 2]
    );
}

#[test]
fn worker_subprocess_exits_cleanly_when_stdin_closes_with_no_requests() {
    let (out, status) = run_session(&[]);
    assert!(status.success(), "an empty session must exit 0");
    assert!(
        out.is_empty(),
        "the worker must write nothing when no requests were sent"
    );
}

#[test]
fn worker_subprocess_stops_on_a_malformed_frame_with_a_nonzero_exit() {
    let mut input = encode_requests(&[(1, parse_request(SourceLanguage::Python, b"x = 1"))]);
    input.extend_from_slice(b"XXXX\x01\x00\x00\x00\x00\x00\x01\x00\x00\x00\x00");
    let (out, status) = run_session(&input);

    assert!(
        !status.success(),
        "a malformed frame must cause a non-zero worker exit"
    );
    let responses = decode_all_responses(&out);
    assert_eq!(
        responses.len(),
        1,
        "only the well-formed first request is served before the malformed frame"
    );
}

#[test]
fn internal_worker_flag_wins_over_an_argument_clap_would_reject() {
    let mut child = spawn_worker_with_args(&["--not-a-real-clap-flag"]);

    let input = encode_requests(&[(1, parse_request(SourceLanguage::Python, b"print(1)"))]);
    {
        let mut stdin = child.stdin.take().expect("stdin must be piped");
        stdin
            .write_all(&input)
            .expect("writing requests to the worker must succeed");
    }
    let mut stdout = child.stdout.take().expect("stdout must be piped");
    let mut out = Vec::new();
    stdout
        .read_to_end(&mut out)
        .expect("reading worker stdout must succeed");
    let status = child.wait().expect("the worker must terminate");

    assert!(
        status.success(),
        "the worker flag must be detected before clap sees the unrecognized argument"
    );
    let responses = decode_all_responses(&out);
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0].message, Response::Parsed { error_count: 0 });
}

#[test]
fn worker_subprocess_serves_a_bounded_sequence_over_real_pipes() {
    let input = encode_requests(&[
        (10, parse_request(SourceLanguage::Python, b"a = 1")),
        (
            11,
            parse_request(SourceLanguage::JavaScript, b"const x = 1;"),
        ),
        (12, parse_request(SourceLanguage::Bash, b"echo hi")),
    ]);
    let (out, status) = run_session(&input);

    assert!(status.success());
    let responses = decode_all_responses(&out);
    assert_eq!(
        responses.iter().map(|f| f.request_id).collect::<Vec<_>>(),
        vec![10, 11, 12]
    );
}
