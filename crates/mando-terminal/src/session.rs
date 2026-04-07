use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use tokio::sync::broadcast;
use tracing::{debug, warn};

use crate::types::{Agent, SessionId, SessionInfo, TerminalEvent, TerminalSize};

pub struct TerminalSession {
    id: SessionId,
    project: String,
    cwd: PathBuf,
    agent: Agent,
    writer: std::sync::Mutex<Box<dyn Write + Send>>,
    master: std::sync::Mutex<Box<dyn MasterPty + Send>>,
    output_tx: broadcast::Sender<TerminalEvent>,
    /// Ring buffer of recent output for replay to late subscribers.
    output_buf: Arc<std::sync::Mutex<Vec<u8>>>,
    running: Arc<AtomicBool>,
    exit_code: Arc<std::sync::Mutex<Option<u32>>>,
    killer: std::sync::Mutex<Box<dyn ChildKiller + Send + Sync>>,
}

impl TerminalSession {
    pub fn spawn(
        id: SessionId,
        project: String,
        cwd: PathBuf,
        agent: Agent,
        resume_session_id: Option<&str>,
        size: TerminalSize,
    ) -> anyhow::Result<Arc<Self>> {
        let pty_system = native_pty_system();
        let pty_size = PtySize {
            rows: size.rows,
            cols: size.cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        let pair = pty_system.openpty(pty_size)?;

        let mut cmd = match &agent {
            Agent::Claude => {
                let mut c = CommandBuilder::new("claude");
                if let Some(sid) = resume_session_id {
                    c.arg("--resume");
                    c.arg(sid);
                }
                c
            }
            Agent::Codex => CommandBuilder::new("codex"),
        };
        cmd.cwd(&cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("MANDO_TERMINAL", "1");

        let mut child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let killer = child.clone_killer();
        let reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let (output_tx, _) = broadcast::channel(4096);
        let output_buf: Arc<std::sync::Mutex<Vec<u8>>> =
            Arc::new(std::sync::Mutex::new(Vec::with_capacity(64 * 1024)));
        let running = Arc::new(AtomicBool::new(true));
        let exit_code: Arc<std::sync::Mutex<Option<u32>>> = Arc::new(std::sync::Mutex::new(None));

        let session = Arc::new(Self {
            id: id.clone(),
            project,
            cwd,
            agent,
            writer: std::sync::Mutex::new(writer),
            master: std::sync::Mutex::new(pair.master),
            output_tx: output_tx.clone(),
            output_buf: output_buf.clone(),
            running: running.clone(),
            exit_code: exit_code.clone(),
            killer: std::sync::Mutex::new(killer),
        });

        // Output reader thread: reads PTY stdout, broadcasts to subscribers.
        let tx = output_tx.clone();
        let run = running.clone();
        let sid = id.clone();
        let buf_clone = output_buf;
        std::thread::Builder::new()
            .name(format!("pty-read-{sid}"))
            .spawn(move || read_pty_output(reader, tx, buf_clone, run, &sid))?;

        // Child waiter thread: waits for process exit, broadcasts Exit event.
        let exit_arc = exit_code;
        let run2 = running;
        let sid2 = id;
        std::thread::Builder::new()
            .name(format!("pty-wait-{sid2}"))
            .spawn(move || match child.wait() {
                Ok(status) => {
                    let code = status.exit_code();
                    *exit_arc.lock().expect("exit_code lock") = Some(code);
                    run2.store(false, Ordering::SeqCst);
                    let _ = output_tx.send(TerminalEvent::Exit { code: Some(code) });
                    debug!(session = sid2, code, "terminal process exited");
                }
                Err(e) => {
                    warn!(session = sid2, error = %e, "terminal wait() failed");
                    run2.store(false, Ordering::SeqCst);
                    let _ = output_tx.send(TerminalEvent::Exit { code: None });
                }
            })?;

        Ok(session)
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn write_input(&self, data: &[u8]) -> anyhow::Result<()> {
        let mut writer = self.writer.lock().expect("writer lock");
        writer.write_all(data)?;
        writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, size: TerminalSize) -> anyhow::Result<()> {
        let master = self.master.lock().expect("master lock");
        master.resize(PtySize {
            rows: size.rows,
            cols: size.cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TerminalEvent> {
        self.output_tx.subscribe()
    }

    /// Returns buffered output for replay to late subscribers.
    pub fn snapshot(&self) -> Vec<u8> {
        self.output_buf.lock().expect("output_buf lock").clone()
    }

    pub fn kill(&self) -> anyhow::Result<()> {
        let mut killer = self.killer.lock().expect("killer lock");
        killer.kill()?;
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn info(&self) -> SessionInfo {
        SessionInfo {
            id: self.id.clone(),
            project: self.project.clone(),
            cwd: self.cwd.clone(),
            agent: self.agent.clone(),
            running: self.is_running(),
            exit_code: *self.exit_code.lock().expect("exit_code lock"),
        }
    }
}

const MAX_OUTPUT_BUF: usize = 64 * 1024;

fn read_pty_output(
    mut reader: Box<dyn Read + Send>,
    tx: broadcast::Sender<TerminalEvent>,
    output_buf: Arc<std::sync::Mutex<Vec<u8>>>,
    running: Arc<AtomicBool>,
    session_id: &str,
) {
    let mut buf = [0u8; 8192];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let chunk = &buf[..n];
                // Append to replay buffer (cap at MAX_OUTPUT_BUF, keep tail).
                {
                    let mut ob = output_buf.lock().expect("output_buf lock");
                    ob.extend_from_slice(chunk);
                    if ob.len() > MAX_OUTPUT_BUF {
                        let drain = ob.len() - MAX_OUTPUT_BUF;
                        ob.drain(..drain);
                    }
                }
                if tx.send(TerminalEvent::Output(chunk.to_vec())).is_err() {
                    // No subscribers; keep reading to avoid PTY backpressure.
                }
            }
            Err(e) => {
                if running.load(Ordering::SeqCst) {
                    warn!(session = session_id, error = %e, "pty read error");
                }
                break;
            }
        }
    }
    debug!(session = session_id, "pty reader finished");
}
