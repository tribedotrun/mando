use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use portable_pty::{CommandBuilder, PtySize};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, warn};

use crate::types::{Agent, CreateRequest, TerminalEvent, TerminalSize};

const MAX_OUTPUT_BUF: usize = 2 * 1024 * 1024;
const OUTPUT_BATCH_SIZE: usize = 16 * 1024;
const OUTPUT_BATCH_WINDOW: Duration = Duration::from_millis(16);

pub(super) struct BatcherThreadCtx {
    pub session_id: String,
    pub raw_rx: std::sync::mpsc::Receiver<Vec<u8>>,
    pub output_tx: broadcast::Sender<TerminalEvent>,
    pub output_buf: Arc<std::sync::Mutex<Vec<u8>>>,
    pub running: Arc<AtomicBool>,
    pub wait_finished: Arc<AtomicBool>,
    pub exit_code: Arc<std::sync::Mutex<Option<u32>>>,
}

pub(super) fn build_command(
    agent: &Agent,
    cwd: &PathBuf,
    resume_session_id: Option<&str>,
    extra_args: &[String],
) -> CommandBuilder {
    let mut cmd = match agent {
        Agent::Claude => {
            let mut builder = CommandBuilder::new("claude");
            for arg in extra_args {
                builder.arg(arg);
            }
            if let Some(session_id) = resume_session_id.filter(|s| !s.is_empty()) {
                builder.arg("--resume");
                builder.arg(session_id);
            }
            builder
        }
        Agent::Codex => {
            let mut builder = CommandBuilder::new("codex");
            for arg in extra_args {
                builder.arg(arg);
            }
            builder
        }
    };
    cmd.cwd(cwd);
    cmd
}

pub(super) fn terminal_env(req: &CreateRequest) -> HashMap<String, String> {
    let mut env = req.terminal_env.clone();
    env.entry("TERM".into())
        .or_insert_with(|| "xterm-256color".into());
    env.entry("PWD".into())
        .or_insert_with(|| req.cwd.to_string_lossy().into_owned());
    env.entry("MANDO_TERMINAL".into())
        .or_insert_with(|| "1".into());
    env
}

pub(super) fn pty_size(size: TerminalSize) -> PtySize {
    PtySize {
        rows: size.rows,
        cols: size.cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

pub(super) fn spawn_writer_thread(
    session_id: String,
    mut writer: Box<dyn Write + Send>,
    mut input_rx: mpsc::Receiver<Vec<u8>>,
) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .name(format!("pty-write-{session_id}"))
        .spawn(move || {
            while let Some(chunk) = input_rx.blocking_recv() {
                if let Err(err) = writer.write_all(&chunk).and_then(|_| writer.flush()) {
                    warn!(session = session_id, error = %err, "pty write failed");
                    break;
                }
            }
        })?;
    Ok(())
}

pub(super) fn spawn_reader_thread(
    session_id: String,
    mut reader: Box<dyn Read + Send>,
    raw_tx: std::sync::mpsc::Sender<Vec<u8>>,
    running: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .name(format!("pty-read-{session_id}"))
        .spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if raw_tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        if running.load(Ordering::SeqCst) {
                            warn!(session = session_id, error = %err, "pty read error");
                        }
                        break;
                    }
                }
            }
            debug!(session = session_id, "pty reader finished");
        })?;
    Ok(())
}

pub(super) fn spawn_batcher_thread(ctx: BatcherThreadCtx) -> anyhow::Result<()> {
    let BatcherThreadCtx {
        session_id,
        raw_rx,
        output_tx,
        output_buf,
        running,
        wait_finished,
        exit_code,
    } = ctx;
    std::thread::Builder::new()
        .name(format!("pty-batch-{session_id}"))
        .spawn(move || {
            let mut pending = Vec::with_capacity(OUTPUT_BATCH_SIZE);
            loop {
                match raw_rx.recv_timeout(OUTPUT_BATCH_WINDOW) {
                    Ok(chunk) => {
                        pending.extend_from_slice(&chunk);
                        if pending.len() >= OUTPUT_BATCH_SIZE {
                            flush_output(&pending, &output_tx, &output_buf);
                            pending.clear();
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        if !pending.is_empty() {
                            flush_output(&pending, &output_tx, &output_buf);
                            pending.clear();
                        }
                        if !running.load(Ordering::SeqCst) && wait_finished.load(Ordering::SeqCst) {
                            break;
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        if !pending.is_empty() {
                            flush_output(&pending, &output_tx, &output_buf);
                            pending.clear();
                        }
                        while !wait_finished.load(Ordering::SeqCst) {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        break;
                    }
                }
            }

            let code = *exit_code.lock().expect("exit_code lock");
            let _ = output_tx.send(TerminalEvent::Exit { code });
        })?;
    Ok(())
}

fn flush_output(
    data: &[u8],
    output_tx: &broadcast::Sender<TerminalEvent>,
    output_buf: &Arc<std::sync::Mutex<Vec<u8>>>,
) {
    {
        let mut buf = output_buf.lock().expect("output_buf lock");
        buf.extend_from_slice(data);
        if buf.len() > MAX_OUTPUT_BUF {
            let drain = buf.len() - MAX_OUTPUT_BUF;
            buf.drain(..drain);
        }
    }
    let _ = output_tx.send(TerminalEvent::Output(data.to_vec()));
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use crate::types::{Agent, CreateRequest, TerminalSize};

    use super::terminal_env;

    #[test]
    fn terminal_env_keeps_explicit_values() {
        let env = terminal_env(&CreateRequest {
            project: "mando".into(),
            cwd: PathBuf::from("/tmp/mando"),
            agent: Agent::Claude,
            resume_session_id: None,
            size: Some(TerminalSize { rows: 24, cols: 80 }),
            config_env: HashMap::new(),
            terminal_env: HashMap::from([
                ("TERM".into(), "vt100".into()),
                ("PWD".into(), "/custom".into()),
                ("MANDO_TERMINAL".into(), "custom".into()),
            ]),
            terminal_id: None,
            extra_args: Vec::new(),
            name: None,
        });

        assert_eq!(env.get("TERM"), Some(&"vt100".to_string()));
        assert_eq!(env.get("PWD"), Some(&"/custom".to_string()));
        assert_eq!(env.get("MANDO_TERMINAL"), Some(&"custom".to_string()));
    }
}
