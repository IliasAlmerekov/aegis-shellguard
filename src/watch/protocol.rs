use std::io::Write;

use aegis_types::SandboxStatus;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader as TokioBufReader};

pub const MAX_FRAME_BYTES: usize = 1 << 20;

#[derive(Debug, Deserialize)]
pub struct InputFrame {
    pub cmd: String,
    pub cwd: Option<String>,
    pub interactive: Option<bool>,
    pub source: Option<String>,
    pub id: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OutputDecision {
    Approved,
    Denied,
    Blocked,
    Error,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum OutputFrame {
    Warning {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        code: &'static str,
        sandbox_status: SandboxStatus,
        message: &'static str,
    },
    Stdout {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        data_b64: String,
    },
    Stderr {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        data_b64: String,
    },
    Result {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        decision: OutputDecision,
        exit_code: i32,
    },
    #[serde(rename = "result")]
    SandboxResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        decision: OutputDecision,
        exit_code: i32,
        code: &'static str,
        sandbox_status: SandboxStatus,
        message: &'static str,
    },
    Error {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        exit_code: i32,
        message: String,
    },
}

pub enum ReadLineResult {
    Line(String),
    Oversized,
    Eof,
}

pub async fn read_bounded_line<R>(
    reader: &mut TokioBufReader<R>,
    max_bytes: usize,
) -> std::io::Result<ReadLineResult>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut buf: Vec<u8> = Vec::new();

    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            if buf.is_empty() {
                return Ok(ReadLineResult::Eof);
            }
            return to_utf8_line(buf);
        }

        let newline_pos = available.iter().position(|&b| b == b'\n');
        let chunk_len = newline_pos.map_or(available.len(), |p| p + 1);
        let is_end = newline_pos.is_some();
        let content_chunk_len = if let Some(pos) = newline_pos {
            let has_cr = pos > 0 && available[pos - 1] == b'\r';
            pos - usize::from(has_cr)
        } else {
            chunk_len
        };

        if buf.len() + content_chunk_len > max_bytes {
            reader.consume(chunk_len);
            if !is_end {
                drain_to_newline(reader).await?;
            }
            return Ok(ReadLineResult::Oversized);
        }

        if is_end {
            buf.extend_from_slice(&available[..chunk_len - 1]);
        } else {
            buf.extend_from_slice(&available[..chunk_len]);
        }
        reader.consume(chunk_len);

        if is_end {
            if buf.last() == Some(&b'\r') {
                buf.pop();
            }
            return to_utf8_line(buf);
        }
    }
}

fn to_utf8_line(buf: Vec<u8>) -> std::io::Result<ReadLineResult> {
    String::from_utf8(buf)
        .map(ReadLineResult::Line)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

async fn drain_to_newline<R>(reader: &mut TokioBufReader<R>) -> std::io::Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
{
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return Ok(());
        }
        if let Some(p) = available.iter().position(|&b| b == b'\n') {
            reader.consume(p + 1);
            return Ok(());
        }
        let len = available.len();
        reader.consume(len);
    }
}

pub fn emit_frame(frame: &OutputFrame) -> std::io::Result<()> {
    let line = serde_json::to_string(frame).map_err(std::io::Error::other)?;
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    lock.write_all(line.as_bytes())?;
    lock.write_all(b"\n")?;
    lock.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

    async fn read_line(input: &[u8]) -> std::io::Result<ReadLineResult> {
        let mut reader = TokioBufReader::new(input);
        read_bounded_line(&mut reader, MAX_FRAME_BYTES).await
    }

    async fn read_line_with_limit(input: &[u8], limit: usize) -> std::io::Result<ReadLineResult> {
        let mut reader = TokioBufReader::new(input);
        read_bounded_line(&mut reader, limit).await
    }

    #[tokio::test]
    async fn read_line_basic() {
        let result = read_line(b"{\"cmd\":\"ls\"}\n").await.unwrap();
        match result {
            ReadLineResult::Line(s) => assert_eq!(s, "{\"cmd\":\"ls\"}"),
            _ => panic!("expected Line"),
        }
    }

    #[tokio::test]
    async fn read_line_eof_returns_eof() {
        let result = read_line(b"").await.unwrap();
        assert!(matches!(result, ReadLineResult::Eof));
    }

    #[tokio::test]
    async fn read_line_no_trailing_newline_returns_line() {
        let result = read_line(b"{\"cmd\":\"ls\"}").await.unwrap();
        match result {
            ReadLineResult::Line(s) => assert_eq!(s, "{\"cmd\":\"ls\"}"),
            _ => panic!("expected Line"),
        }
    }

    #[tokio::test]
    async fn read_line_oversized_returns_oversized() {
        let result = read_line_with_limit(b"1234567\n", 5).await.unwrap();
        assert!(matches!(result, ReadLineResult::Oversized));
    }

    #[tokio::test]
    async fn read_line_at_exact_limit_with_newline_returns_line() {
        let result = read_line_with_limit(b"12345\n", 5).await.unwrap();
        match result {
            ReadLineResult::Line(s) => assert_eq!(s, "12345"),
            _ => panic!("expected Line"),
        }
    }

    #[tokio::test]
    async fn read_line_oversized_then_next_line_ok() {
        let input = b"1234567\nnext\n";
        let mut reader = TokioBufReader::new(input.as_ref());
        let first = read_bounded_line(&mut reader, 5).await.unwrap();
        assert!(matches!(first, ReadLineResult::Oversized));
        let second = read_bounded_line(&mut reader, 5).await.unwrap();
        match second {
            ReadLineResult::Line(s) => assert_eq!(s, "next"),
            _ => panic!("expected Line for second frame"),
        }
    }

    #[tokio::test]
    async fn read_line_strips_crlf() {
        let result = read_line(b"{\"cmd\":\"ls\"}\r\n").await.unwrap();
        match result {
            ReadLineResult::Line(s) => assert_eq!(s, "{\"cmd\":\"ls\"}"),
            _ => panic!("expected Line"),
        }
    }

    #[test]
    fn output_frame_result_serializes_correctly() {
        let frame = OutputFrame::Result {
            id: Some("42".to_string()),
            decision: OutputDecision::Approved,
            exit_code: 0,
        };
        let json = serde_json::to_string(&frame).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["type"], "result");
        assert_eq!(v["id"], "42");
        assert_eq!(v["decision"], "approved");
        assert_eq!(v["exit_code"], 0);
    }

    #[test]
    fn output_frame_result_omits_id_when_none() {
        let frame = OutputFrame::Result {
            id: None,
            decision: OutputDecision::Denied,
            exit_code: 2,
        };
        let json = serde_json::to_string(&frame).unwrap();
        assert!(!json.contains("\"id\""), "id must be absent when None");
    }

    #[test]
    fn required_sandbox_block_serializes_diagnostics_on_result() {
        let frame = OutputFrame::SandboxResult {
            id: Some("sandbox-2".to_string()),
            decision: OutputDecision::Blocked,
            exit_code: 3,
            code: crate::runtime::SANDBOX_REQUIRED_UNAVAILABLE_CODE,
            sandbox_status: SandboxStatus::Unavailable,
            message: crate::runtime::SANDBOX_REQUIRED_UNAVAILABLE_MESSAGE,
        };

        let value = serde_json::to_value(frame).unwrap();

        assert_eq!(value["type"], "result");
        assert_eq!(value["code"], "sandbox_required_unavailable");
        assert_eq!(value["sandbox_status"], "unavailable");
        assert_eq!(value["exit_code"], 3);
    }

    #[test]
    fn required_nested_sandbox_block_serializes_diagnostics_on_result() {
        let frame = OutputFrame::SandboxResult {
            id: Some("sandbox-3".to_string()),
            decision: OutputDecision::Blocked,
            exit_code: 3,
            code: crate::runtime::SANDBOX_REQUIRED_NESTED_UNAVAILABLE_CODE,
            sandbox_status: SandboxStatus::Unavailable,
            message: crate::runtime::SANDBOX_REQUIRED_NESTED_UNAVAILABLE_MESSAGE,
        };

        let value = serde_json::to_value(frame).unwrap();

        assert_eq!(value["type"], "result");
        assert_eq!(value["code"], "sandbox_required_nested_unavailable");
        assert_eq!(value["sandbox_status"], "unavailable");
        assert_eq!(value["exit_code"], 3);
    }

    #[test]
    fn output_frame_stdout_uses_base64() {
        let data = b"\xff\xfe";
        let frame = OutputFrame::Stdout {
            id: None,
            data_b64: BASE64.encode(data),
        };
        let json = serde_json::to_string(&frame).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["type"], "stdout");
        let decoded = BASE64.decode(v["data_b64"].as_str().unwrap()).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn output_frame_error_serializes_correctly() {
        let frame = OutputFrame::Error {
            id: Some("bad".to_string()),
            exit_code: 4,
            message: "invalid JSON".to_string(),
        };
        let json = serde_json::to_string(&frame).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["type"], "error");
        assert_eq!(v["exit_code"], 4);
        assert_eq!(v["message"], "invalid JSON");
    }
}
