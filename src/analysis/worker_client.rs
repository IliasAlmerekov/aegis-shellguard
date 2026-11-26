use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time::timeout;

use aegis_language::SourceLanguage;
use aegis_language::protocol::{self, DecodeError, Request, Response};
use aegis_types::DegradationReason;

pub const INTERNAL_LANGUAGE_WORKER_FLAG: &str = "--internal-language-worker";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestKind {
    Parse,
    Analyze,
}

#[derive(Debug, Clone)]
pub struct TargetRequest {
    pub request_id: u32,
    pub language: SourceLanguage,
    pub source: Vec<u8>,
    pub kind: RequestKind,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum WorkerError {
    #[error("language-analysis deadline exceeded")]
    Timeout,
    #[error("worker closed its output before all responses arrived")]
    Closed,
    #[error("worker stdout contained a malformed frame: {0}")]
    ProtocolNoise(#[from] DecodeError),
    #[error("duplicate response for request id {request_id}")]
    DuplicateResponse { request_id: u32 },
    #[error("out-of-order response: expected {expected}, got {got}")]
    OutOfOrder { expected: u32, got: u32 },
    #[error("unexpected response for request id {request_id} (not in flight)")]
    UnexpectedResponse { request_id: u32 },
    #[error("worker exited with non-zero status: {code:?}")]
    NonZeroExit { code: Option<i32> },
    #[error("worker i/o error: {message}")]
    Io {
        kind: std::io::ErrorKind,
        message: String,
    },
}

impl WorkerError {
    fn from_io(error: std::io::Error) -> Self {
        WorkerError::Io {
            kind: error.kind(),
            message: error.to_string(),
        }
    }
}

impl From<WorkerError> for DegradationReason {
    fn from(_: WorkerError) -> Self {
        DegradationReason::WorkerFailure
    }
}

#[derive(Debug, Clone)]
pub enum TargetResult {
    Responded(Response),
    Failed(WorkerError),
}

pub async fn analyze<R, W>(
    reader: &mut R,
    writer: &mut W,
    requests: Vec<TargetRequest>,
    deadline: Duration,
) -> Vec<TargetResult>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    let n = requests.len();
    let sent_ids: Vec<u32> = requests.iter().map(|r| r.request_id).collect();
    let mut results: Vec<Option<TargetResult>> = vec![None; n];

    if let Err(fail) = send_requests(writer, &requests).await {
        return all_failed(n, fail);
    }

    let outcome = timeout(deadline, read_responses(reader, &sent_ids, &mut results)).await;
    let session_failure: Option<WorkerError> = match outcome {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(error),
        Err(_) => Some(WorkerError::Timeout),
    };

    if let Some(fail) = session_failure {
        for slot in results.iter_mut() {
            if slot.is_none() {
                *slot = Some(TargetResult::Failed(fail.clone()));
            }
        }
    }

    results
        .into_iter()
        .map(|slot| slot.unwrap_or(TargetResult::Failed(WorkerError::Closed)))
        .collect()
}

async fn send_requests<W>(writer: &mut W, requests: &[TargetRequest]) -> Result<(), WorkerError>
where
    W: AsyncWrite + Unpin + Send,
{
    let mut out = Vec::new();
    for req in requests {
        let request = match req.kind {
            RequestKind::Parse => Request::Parse {
                language: req.language,
                source: req.source.clone(),
            },
            RequestKind::Analyze => Request::Analyze {
                language: req.language,
                source: req.source.clone(),
            },
        };
        let frame =
            protocol::encode_request(req.request_id, &request).map_err(|_| WorkerError::Closed)?;
        out.extend_from_slice(&frame);
    }
    if out.is_empty() {
        return Ok(());
    }
    writer.write_all(&out).await.map_err(WorkerError::from_io)?;
    writer.flush().await.map_err(WorkerError::from_io)?;
    Ok(())
}

fn all_failed(n: usize, fail: WorkerError) -> Vec<TargetResult> {
    (0..n).map(|_| TargetResult::Failed(fail.clone())).collect()
}

async fn read_responses<R>(
    reader: &mut R,
    sent_ids: &[u32],
    results: &mut [Option<TargetResult>],
) -> Result<(), WorkerError>
where
    R: AsyncRead + Unpin + Send,
{
    let n = sent_ids.len();
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut expected = 0usize;

    while expected < n {
        let frame = loop {
            match protocol::decode_response(&buf) {
                Ok(Some(frame)) => {
                    buf.drain(..frame.consumed);
                    break frame;
                }
                Ok(None) => match reader.read(&mut chunk).await {
                    Ok(0) => return Err(WorkerError::Closed),
                    Ok(k) => buf.extend_from_slice(&chunk[..k]),
                    Err(e) => return Err(WorkerError::from_io(e)),
                },
                Err(e) => return Err(WorkerError::ProtocolNoise(e)),
            }
        };

        let id = frame.request_id;
        if id == sent_ids[expected] {
            results[expected] = Some(TargetResult::Responded(frame.message));
            expected += 1;
        } else if sent_ids[expected..].contains(&id) {
            return Err(WorkerError::OutOfOrder {
                expected: sent_ids[expected],
                got: id,
            });
        } else if sent_ids[..expected].contains(&id) {
            return Err(WorkerError::DuplicateResponse { request_id: id });
        } else {
            return Err(WorkerError::UnexpectedResponse { request_id: id });
        }
    }
    Ok(())
}

pub struct Worker {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: Option<ChildStdout>,
    exit_code: Option<i32>,
    reaped: bool,
}

impl Worker {
    pub async fn spawn(aegis_path: Option<&str>) -> std::io::Result<Worker> {
        let mut cmd = match aegis_path {
            Some(path) => Command::new(path),
            None => {
                let exe = std::env::current_exe()?;
                Command::new(exe)
            }
        };
        cmd.arg(INTERNAL_LANGUAGE_WORKER_FLAG)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        Self::spawn_command(cmd)
    }

    pub fn spawn_command(mut cmd: Command) -> std::io::Result<Worker> {
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        Ok(Worker {
            child,
            stdin,
            stdout,
            exit_code: None,
            reaped: false,
        })
    }

    pub async fn analyze(
        &mut self,
        requests: Vec<TargetRequest>,
        deadline: Duration,
    ) -> Vec<TargetResult> {
        let n = requests.len();
        let sent_ids: Vec<u32> = requests.iter().map(|r| r.request_id).collect();
        let mut results: Vec<Option<TargetResult>> = vec![None; n];
        let mut all_responses_received = false;

        let session = async {
            match self.stdin.as_mut() {
                Some(writer) => send_requests(writer, &requests).await?,
                None => return Err(WorkerError::Closed),
            }
            self.stdin.take();

            match self.stdout.as_mut() {
                Some(reader) => read_responses(reader, &sent_ids, &mut results).await?,
                None => return Err(WorkerError::Closed),
            }
            all_responses_received = true;

            let code = self
                .child
                .wait()
                .await
                .ok()
                .and_then(|status| status.code());
            self.reaped = true;
            self.exit_code = code;
            if code != Some(0) {
                return Err(WorkerError::NonZeroExit { code });
            }
            Ok(())
        };

        let failure = match timeout(deadline, session).await {
            Ok(Ok(())) => None,
            Ok(Err(fail)) => Some(fail),
            Err(_) => Some(WorkerError::Timeout),
        };

        if let Some(fail) = failure {
            if !self.reaped {
                self.reap_kill().await;
            }
            if all_responses_received || matches!(fail, WorkerError::NonZeroExit { .. }) {
                return all_failed(n, fail);
            }
            for slot in &mut results {
                if slot.is_none() {
                    *slot = Some(TargetResult::Failed(fail.clone()));
                }
            }
        }

        results
            .into_iter()
            .map(|slot| slot.unwrap_or(TargetResult::Failed(WorkerError::Closed)))
            .collect()
    }

    #[must_use]
    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    pub async fn shutdown(mut self) -> std::io::Result<Option<i32>> {
        self.stdin.take();
        self.stdout.take();
        if !self.reaped {
            let status = self.child.wait().await?;
            self.exit_code = status.code();
            self.reaped = true;
        }
        Ok(self.exit_code)
    }

    async fn reap_kill(&mut self) {
        let _ = self.child.start_kill();
        if let Ok(status) = self.child.wait().await {
            self.exit_code = status.code();
        }
        self.reaped = true;
    }
}
