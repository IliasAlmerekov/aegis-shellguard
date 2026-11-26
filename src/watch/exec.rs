use tokio::io::{AsyncReadExt, BufReader as TokioBufReader};
use tokio::sync::mpsc;

use aegis_types::EXIT_INTERNAL;

use super::protocol::{
    OutputDecision, OutputFrame, emit_error_or_exit, emit_frame, emit_result_or_exit, exit_internal,
};

const CHANNEL_CAPACITY: usize = 64;

enum WatchEvent {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
}

pub(super) async fn execute_prepared_and_emit(
    mut command: std::process::Command,
    cwd: Option<&std::path::Path>,
    id: Option<String>,
) {
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    use tokio::process::Command;

    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = match Command::from(command).spawn() {
        Ok(c) => c,
        Err(e) => {
            emit_error_or_exit(id, format!("failed to spawn child: {e}"));
            return;
        }
    };

    let (Some(child_stdout), Some(child_stderr)) = (child.stdout.take(), child.stderr.take())
    else {
        let message = match child.start_kill() {
            Ok(()) => "failed to capture child output".to_owned(),
            Err(e) => format!("failed to capture child output; failed to kill child: {e}"),
        };
        emit_error_or_exit(id, message);
        return;
    };

    let (tx, mut rx) = mpsc::channel::<WatchEvent>(CHANNEL_CAPACITY);

    let tx_out = tx.clone();
    tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        let mut reader = TokioBufReader::new(child_stdout);
        loop {
            match reader.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx_out
                        .send(WatchEvent::Stdout(buf[..n].to_vec()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    });

    let tx_err = tx;
    tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        let mut reader = TokioBufReader::new(child_stderr);
        loop {
            match reader.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx_err
                        .send(WatchEvent::Stderr(buf[..n].to_vec()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    });

    while let Some(event) = rx.recv().await {
        use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
        let frame = match event {
            WatchEvent::Stdout(data) => OutputFrame::Stdout {
                id: id.clone(),
                data_b64: BASE64.encode(&data),
            },
            WatchEvent::Stderr(data) => OutputFrame::Stderr {
                id: id.clone(),
                data_b64: BASE64.encode(&data),
            },
        };
        if emit_frame(&frame).is_err() {
            let _ = child.kill().await;
            exit_internal();
        }
    }

    let exit_code = match child.wait().await {
        Ok(status) => status.code().unwrap_or_else(|| {
            #[cfg(unix)]
            {
                128 + status.signal().unwrap_or(0)
            }
            #[cfg(not(unix))]
            {
                128
            }
        }),
        Err(_) => EXIT_INTERNAL,
    };

    emit_result_or_exit(id, OutputDecision::Approved, exit_code);
}
