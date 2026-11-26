use std::io::{Read, Write};

use crate::language::{self, SourceLanguage};
use crate::protocol::{self, Request, Response};
use crate::router::{self, SourceTarget};

pub const MAX_REQUESTS_PER_SESSION: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    EndOfInput,
    MaxRequestsReached,
    TruncatedFrame,
    MalformedFrame,
    ReadFailed,
    WriteFailed,
}

pub fn run<R: Read, W: Write>(reader: R, writer: W) -> RunOutcome {
    run_with_limit(reader, writer, MAX_REQUESTS_PER_SESSION)
}

pub(crate) fn run_with_limit<R: Read, W: Write>(
    mut reader: R,
    mut writer: W,
    max_requests: u32,
) -> RunOutcome {
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut served: u32 = 0;
    loop {
        match protocol::decode_request(&buf) {
            Ok(Some(frame)) => {
                buf.drain(..frame.consumed);
                let response = handle_request(&frame.message);
                let encoded = match protocol::encode_response(frame.request_id, &response) {
                    Ok(bytes) => bytes,
                    Err(_) => return RunOutcome::WriteFailed,
                };
                if writer.write_all(&encoded).is_err() || writer.flush().is_err() {
                    return RunOutcome::WriteFailed;
                }
                served += 1;
                if served >= max_requests {
                    return RunOutcome::MaxRequestsReached;
                }
            }
            Ok(None) => match reader.read(&mut chunk) {
                Ok(0) => {
                    return if buf.is_empty() {
                        RunOutcome::EndOfInput
                    } else {
                        RunOutcome::TruncatedFrame
                    };
                }
                Ok(n) => buf.extend_from_slice(&chunk[..n]),
                Err(_) => return RunOutcome::ReadFailed,
            },
            Err(_) => return RunOutcome::MalformedFrame,
        }
    }
}

fn handle_request(request: &Request) -> Response {
    let (language, source) = match request {
        Request::Parse { language, source } => (*language, source.as_slice()),
        Request::Analyze { language, source } => {
            return analyze_source(language, source);
        }
    };
    let source_str = match std::str::from_utf8(source) {
        Ok(s) => s,
        Err(_) => return Response::ParseFailed,
    };
    match language::parse(language, source_str) {
        Ok(tree) => Response::Parsed {
            error_count: u32::from(tree.root_node().has_error()),
        },
        Err(_) => Response::ParseFailed,
    }
}

fn analyze_source(language: &SourceLanguage, source: &[u8]) -> Response {
    let adapter = match language {
        SourceLanguage::Python => crate::languages::python::analyze,
        SourceLanguage::JavaScript => crate::languages::javascript::analyze,
        SourceLanguage::TypeScript => crate::languages::typescript::analyze,
        SourceLanguage::Bash => crate::languages::bash::analyze,
    };
    let source_str = match std::str::from_utf8(source) {
        Ok(s) => s,
        Err(_) => {
            return Response::Analyzed {
                result: crate::operation::AdapterResult {
                    operations: Vec::new(),
                    parse_errors: 1,
                    degradation: None,
                },
            };
        }
    };
    Response::Analyzed {
        result: adapter(source_str),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    NotStarted,
    Parsed { targets: usize },
}

#[must_use]
pub fn analyze(command: &str) -> Outcome {
    let targets = router::source_targets(command);
    if targets.is_empty() {
        return Outcome::NotStarted;
    }
    Outcome::Parsed {
        targets: count_parseable(targets),
    }
}

fn count_parseable(targets: Vec<SourceTarget>) -> usize {
    targets
        .iter()
        .filter(|t| parses(t.language, &t.source))
        .count()
}

fn parses(language: SourceLanguage, source: &str) -> bool {
    language::parse(language, source).is_ok()
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;
    use crate::protocol::{self, DecodedFrame, Request, Response};

    fn encode_requests(requests: &[(u32, Request)]) -> Vec<u8> {
        let mut buf = Vec::new();
        for (id, req) in requests {
            buf.extend_from_slice(
                &protocol::encode_request(*id, req).expect("test source encodes"),
            );
        }
        buf
    }

    fn decode_responses(buf: &[u8]) -> Vec<DecodedFrame<Response>> {
        let mut out = Vec::new();
        let mut rest = buf;
        while let Some(frame) = protocol::decode_response(rest)
            .expect("worker output must be a sequence of well-formed response frames")
        {
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

    fn analyze_request(language: SourceLanguage, source: &[u8]) -> Request {
        Request::Analyze {
            language,
            source: source.to_vec(),
        }
    }

    #[test]
    fn run_serves_one_clean_parse_request_and_stops_at_end_of_input() {
        let requests = encode_requests(&[(1, parse_request(SourceLanguage::Python, b"print(1)"))]);
        let reader = std::io::Cursor::new(requests);
        let mut output = Vec::new();

        let outcome = run(reader, &mut output);
        assert_eq!(outcome, RunOutcome::EndOfInput);

        let responses = decode_responses(&output);
        assert_eq!(
            responses.len(),
            1,
            "one request must yield exactly one response"
        );
        assert_eq!(responses[0].request_id, 1);
        assert_eq!(
            responses[0].message,
            Response::Parsed { error_count: 0 },
            "clean Python source must parse with zero errors"
        );
    }

    #[test]
    fn run_serves_a_bounded_sequence_then_stops_at_end_of_input() {
        let requests = encode_requests(&[
            (10, parse_request(SourceLanguage::Python, b"x = 1")),
            (11, parse_request(SourceLanguage::Bash, b"echo hi")),
            (
                12,
                parse_request(SourceLanguage::JavaScript, b"const x = 1;"),
            ),
        ]);
        let reader = std::io::Cursor::new(requests);
        let mut output = Vec::new();

        let outcome = run(reader, &mut output);
        assert_eq!(outcome, RunOutcome::EndOfInput);

        let responses = decode_responses(&output);
        assert_eq!(
            responses.iter().map(|f| f.request_id).collect::<Vec<_>>(),
            vec![10, 11, 12],
            "responses must be served in request order"
        );
        for f in &responses {
            assert_eq!(
                f.message,
                Response::Parsed { error_count: 0 },
                "each clean snippet must parse with zero errors"
            );
        }
    }

    #[test]
    fn run_force_exits_after_the_request_cap_without_serving_the_extra() {
        let requests = encode_requests(&[
            (1, parse_request(SourceLanguage::Python, b"a = 1")),
            (2, parse_request(SourceLanguage::Python, b"b = 2")),
            (3, parse_request(SourceLanguage::Python, b"c = 3")),
        ]);
        let reader = std::io::Cursor::new(requests);
        let mut output = Vec::new();

        let outcome = run_with_limit(reader, &mut output, 2);
        assert_eq!(outcome, RunOutcome::MaxRequestsReached);

        let responses = decode_responses(&output);
        assert_eq!(
            responses.iter().map(|f| f.request_id).collect::<Vec<_>>(),
            vec![1, 2],
            "the third request past the cap must not be served"
        );
    }

    #[test]
    fn run_stops_on_a_malformed_frame_without_serving_it() {
        let mut buf = encode_requests(&[(1, parse_request(SourceLanguage::Python, b"x = 1"))]);
        buf.extend_from_slice(b"XXXX\x01\x00\x00\x00\x00\x00\x01\x00\x00\x00\x00");
        let reader = std::io::Cursor::new(buf);
        let mut output = Vec::new();

        let outcome = run(reader, &mut output);
        assert_eq!(outcome, RunOutcome::MalformedFrame);

        let responses = decode_responses(&output);
        assert_eq!(
            responses.len(),
            1,
            "only the well-formed first request is served before the malformed frame"
        );
    }

    #[test]
    fn run_reports_a_truncated_trailing_frame_as_a_worker_failure() {
        let mut buf = encode_requests(&[(1, parse_request(SourceLanguage::Python, b"x = 1"))]);
        buf.extend_from_slice(b"AELW\x01");
        let reader = std::io::Cursor::new(buf);
        let mut output = Vec::new();

        let outcome = run(reader, &mut output);
        assert_eq!(outcome, RunOutcome::TruncatedFrame);

        let responses = decode_responses(&output);
        assert_eq!(
            responses.len(),
            1,
            "the complete first request is still served"
        );
    }

    #[test]
    fn run_reports_parse_failed_for_invalid_utf8_source() {
        let requests = encode_requests(&[(7, parse_request(SourceLanguage::Python, b"\xFF\xFE"))]);
        let reader = std::io::Cursor::new(requests);
        let mut output = Vec::new();

        let outcome = run(reader, &mut output);
        assert_eq!(outcome, RunOutcome::EndOfInput);

        let responses = decode_responses(&output);
        assert_eq!(responses.len(), 1);
        assert_eq!(
            responses[0].message,
            Response::ParseFailed,
            "invalid UTF-8 source must yield ParseFailed, not a panic"
        );
    }

    #[test]
    fn run_reports_a_nonzero_error_count_for_incomplete_syntax() {
        let requests = encode_requests(&[(9, parse_request(SourceLanguage::Python, b"def f("))]);
        let reader = std::io::Cursor::new(requests);
        let mut output = Vec::new();

        let outcome = run(reader, &mut output);
        assert_eq!(outcome, RunOutcome::EndOfInput);

        let responses = decode_responses(&output);
        assert_eq!(responses.len(), 1);
        assert!(
            matches!(responses[0].message, Response::Parsed { error_count } if error_count > 0),
            "incomplete syntax must parse to a tree with a nonzero error count"
        );
    }

    #[test]
    fn run_serves_a_clean_parse_for_each_foundation_grammar() {
        let snippets: &[(SourceLanguage, &[u8])] = &[
            (SourceLanguage::Python, b"print(1)"),
            (SourceLanguage::JavaScript, b"console.log(1);"),
            (SourceLanguage::TypeScript, b"let x: number = 1;"),
            (SourceLanguage::Bash, b"echo hi"),
        ];
        let requests: Vec<(u32, Request)> = snippets
            .iter()
            .enumerate()
            .map(|(i, (lang, src))| (i as u32, parse_request(*lang, src)))
            .collect();
        let encoded = encode_requests(&requests);
        let reader = std::io::Cursor::new(encoded);
        let mut output = Vec::new();

        let outcome = run(reader, &mut output);
        assert_eq!(outcome, RunOutcome::EndOfInput);

        let responses = decode_responses(&output);
        assert_eq!(responses.len(), snippets.len());
        for f in &responses {
            assert_eq!(
                f.message,
                Response::Parsed { error_count: 0 },
                "each foundation grammar must parse its clean snippet with zero errors"
            );
        }
    }

    #[test]
    fn run_analyzes_python_source_and_returns_an_analyzed_response() {
        let requests = encode_requests(&[(
            21,
            analyze_request(SourceLanguage::Python, b"os.remove('x')"),
        )]);
        let reader = std::io::Cursor::new(requests);
        let mut output = Vec::new();

        let outcome = run(reader, &mut output);
        assert_eq!(outcome, RunOutcome::EndOfInput);

        let responses = decode_responses(&output);
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].request_id, 21);
        match &responses[0].message {
            Response::Analyzed { result } => {
                assert_eq!(
                    result.parse_errors, 0,
                    "valid Python must analyze with a clean parse"
                );
                assert!(
                    !result.operations.is_empty(),
                    "a destructive call must surface at least one detected operation"
                );
            }
            other => panic!("Python Analyze must yield Analyzed, got {other:?}"),
        }
    }

    #[test]
    fn run_analyzes_javascript_source_and_returns_an_analyzed_response() {
        let requests = encode_requests(&[(
            24,
            analyze_request(SourceLanguage::JavaScript, b"fs.unlinkSync(\"data.txt\")"),
        )]);
        let reader = std::io::Cursor::new(requests);
        let mut output = Vec::new();

        let outcome = run(reader, &mut output);
        assert_eq!(outcome, RunOutcome::EndOfInput);

        let responses = decode_responses(&output);
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].request_id, 24);
        match &responses[0].message {
            Response::Analyzed { result } => {
                assert_eq!(
                    result.parse_errors, 0,
                    "valid JavaScript must analyze with a clean parse"
                );
                assert!(
                    !result.operations.is_empty(),
                    "a destructive call must surface at least one detected operation"
                );
            }
            other => panic!("JavaScript Analyze must yield Analyzed, got {other:?}"),
        }
    }

    #[test]
    fn run_analyzes_typescript_source_and_returns_an_analyzed_response() {
        let requests = encode_requests(&[(
            25,
            analyze_request(
                SourceLanguage::TypeScript,
                b"fs.unlinkSync<void>(\"data.txt\")",
            ),
        )]);
        let reader = std::io::Cursor::new(requests);
        let mut output = Vec::new();

        let outcome = run(reader, &mut output);
        assert_eq!(outcome, RunOutcome::EndOfInput);

        let responses = decode_responses(&output);
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].request_id, 25);
        match &responses[0].message {
            Response::Analyzed { result } => {
                assert_eq!(
                    result.parse_errors, 0,
                    "valid TypeScript must analyze with a clean parse"
                );
                assert!(
                    !result.operations.is_empty(),
                    "a destructive call must surface at least one detected operation"
                );
            }
            other => panic!("TypeScript Analyze must yield Analyzed, got {other:?}"),
        }
    }

    #[test]
    fn run_analyzes_bash_source_and_returns_an_analyzed_response() {
        let requests =
            encode_requests(&[(22, analyze_request(SourceLanguage::Bash, b"rm -rf build"))]);
        let reader = std::io::Cursor::new(requests);
        let mut output = Vec::new();

        let outcome = run(reader, &mut output);
        assert_eq!(outcome, RunOutcome::EndOfInput);

        let responses = decode_responses(&output);
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].request_id, 22);
        match &responses[0].message {
            Response::Analyzed { result } => {
                assert_eq!(
                    result.parse_errors, 0,
                    "valid shell source must analyze with a clean parse"
                );
                assert!(
                    !result.operations.is_empty(),
                    "a destructive command must surface at least one detected operation"
                );
            }
            other => panic!("Bash Analyze must yield Analyzed, got {other:?}"),
        }
    }

    #[test]
    fn run_returns_an_analyzed_parse_error_for_invalid_utf8_python_source() {
        let requests =
            encode_requests(&[(23, analyze_request(SourceLanguage::Python, b"\xFF\xFE"))]);
        let reader = std::io::Cursor::new(requests);
        let mut output = Vec::new();

        let outcome = run(reader, &mut output);
        assert_eq!(outcome, RunOutcome::EndOfInput);

        let responses = decode_responses(&output);
        assert_eq!(responses.len(), 1);
        match &responses[0].message {
            Response::Analyzed { result } => {
                assert_eq!(
                    result.parse_errors, 1,
                    "invalid UTF-8 must surface as one parse error"
                );
                assert!(
                    result.operations.is_empty(),
                    "no operations may be reported for unparseable bytes"
                );
            }
            other => panic!("invalid-UTF-8 Python Analyze must yield Analyzed, got {other:?}"),
        }
    }
}
