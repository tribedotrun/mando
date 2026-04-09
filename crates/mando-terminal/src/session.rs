use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use portable_pty::{native_pty_system, ChildKiller, MasterPty};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, warn};

use crate::env::ShellEnvResolver;
use crate::history::{TerminalHistoryMeta, TerminalHistoryStore};
use crate::session_io::{
    build_command, pty_size, spawn_batcher_thread, spawn_reader_thread, spawn_writer_thread,
    terminal_env, BatcherThreadCtx,
};
use crate::types::{
    Agent, CreateRequest, SessionId, SessionInfo, SessionState, TerminalEvent, TerminalSize,
};

const MAX_INPUT_BYTES: usize = 512 * 1024;
const INPUT_CHUNK_SIZE: usize = 4096;
const INPUT_QUEUE_CAPACITY: usize = 128;
const MAX_OUTPUT_BUF: usize = 64 * 1024;

struct LiveSession {
    input_tx: mpsc::Sender<Vec<u8>>,
    master: std::sync::Mutex<Box<dyn MasterPty + Send>>,
    killer: std::sync::Mutex<Box<dyn ChildKiller + Send + Sync>>,
    running: Arc<AtomicBool>,
}

struct SpawnedChildGuard {
    session_id: String,
    child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
    killer: Option<Box<dyn ChildKiller + Send + Sync>>,
}

struct WaiterThreadCtx {
    session_id: String,
    child_guard: SpawnedChildGuard,
    history: Arc<TerminalHistoryStore>,
    running: Arc<AtomicBool>,
    state: Arc<std::sync::Mutex<SessionState>>,
    exit_code: Arc<std::sync::Mutex<Option<u32>>>,
    rev: Arc<AtomicU64>,
    ended_at: Arc<std::sync::Mutex<Option<String>>>,
    wait_finished: Arc<AtomicBool>,
}

impl SpawnedChildGuard {
    fn new(
        session_id: String,
        child: Box<dyn portable_pty::Child + Send + Sync>,
        killer: Box<dyn ChildKiller + Send + Sync>,
    ) -> Self {
        Self {
            session_id,
            child: Some(child),
            killer: Some(killer),
        }
    }
}

impl Drop for SpawnedChildGuard {
    fn drop(&mut self) {
        if let Some(killer) = self.killer.as_mut() {
            if let Err(err) = killer.kill() {
                warn!(
                    session = self.session_id,
                    error = %err,
                    "failed to kill terminal child after spawn error"
                );
            }
        }
        if let Some(child) = self.child.as_mut() {
            if let Err(err) = child.wait() {
                warn!(
                    session = self.session_id,
                    error = %err,
                    "failed to wait terminal child after spawn error"
                );
            }
        }
    }
}

pub struct TerminalSession {
    id: SessionId,
    project: String,
    cwd: PathBuf,
    agent: Agent,
    terminal_id: Option<String>,
    output_tx: broadcast::Sender<TerminalEvent>,
    output_buf: Arc<std::sync::Mutex<Vec<u8>>>,
    state: Arc<std::sync::Mutex<SessionState>>,
    exit_code: Arc<std::sync::Mutex<Option<u32>>>,
    rev: Arc<AtomicU64>,
    created_at: String,
    ended_at: Arc<std::sync::Mutex<Option<String>>>,
    live: Option<LiveSession>,
    history: Arc<TerminalHistoryStore>,
}

impl TerminalSession {
    pub fn spawn(
        id: SessionId,
        req: CreateRequest,
        history: Arc<TerminalHistoryStore>,
        env: Arc<ShellEnvResolver>,
    ) -> anyhow::Result<Arc<Self>> {
        let size = req.size.unwrap_or_default();
        let created_at = mando_types::now_rfc3339();
        let meta = TerminalHistoryMeta {
            id: id.clone(),
            project: req.project.clone(),
            cwd: req.cwd.clone(),
            agent: req.agent.clone(),
            terminal_id: req.terminal_id.clone(),
            created_at: created_at.clone(),
            ended_at: None,
            exit_code: None,
            size,
            state: SessionState::Live,
        };

        let pty_system = native_pty_system();
        let pair = pty_system.openpty(pty_size(size))?;
        let output_buf = Arc::new(std::sync::Mutex::new(Vec::with_capacity(MAX_OUTPUT_BUF)));
        let (output_tx, _) = broadcast::channel(4096);
        let running = Arc::new(AtomicBool::new(true));
        let exit_code = Arc::new(std::sync::Mutex::new(None));
        let rev = Arc::new(AtomicU64::new(1));
        let ended_at = Arc::new(std::sync::Mutex::new(None));
        let state = Arc::new(std::sync::Mutex::new(SessionState::Live));

        let terminal_env = terminal_env(&req);
        let merged_env = env.resolve(&req.config_env, &terminal_env);
        let mut cmd = build_command(
            &req.agent,
            &req.cwd,
            req.resume_session_id.as_deref(),
            &req.extra_args,
        );
        cmd.env_clear();
        for (key, value) in merged_env {
            cmd.env(key, value);
        }

        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);
        let live_killer = child.clone_killer();
        let cleanup_killer = child.clone_killer();
        let child_guard = SpawnedChildGuard::new(id.clone(), child, cleanup_killer);

        history.create_session(&meta)?;

        let reader = match pair.master.try_clone_reader() {
            Ok(reader) => reader,
            Err(err) => {
                delete_failed_history(&history, &meta.id);
                return Err(err);
            }
        };
        let writer = match pair.master.take_writer() {
            Ok(writer) => writer,
            Err(err) => {
                delete_failed_history(&history, &meta.id);
                return Err(err);
            }
        };
        let (input_tx, input_rx) = mpsc::channel(INPUT_QUEUE_CAPACITY);
        let (raw_tx, raw_rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let wait_finished = Arc::new(AtomicBool::new(false));

        if let Err(err) = spawn_writer_thread(id.clone(), writer, input_rx) {
            delete_failed_history(&history, &meta.id);
            return Err(err);
        }
        if let Err(err) = spawn_reader_thread(id.clone(), reader, raw_tx, running.clone()) {
            delete_failed_history(&history, &meta.id);
            return Err(err);
        }
        if let Err(err) = spawn_batcher_thread(BatcherThreadCtx {
            session_id: id.clone(),
            raw_rx,
            output_tx: output_tx.clone(),
            output_buf: output_buf.clone(),
            running: running.clone(),
            wait_finished: wait_finished.clone(),
            exit_code: exit_code.clone(),
        }) {
            delete_failed_history(&history, &meta.id);
            return Err(err);
        }
        if let Err(err) = spawn_waiter_thread(WaiterThreadCtx {
            session_id: id,
            child_guard,
            history: history.clone(),
            running: running.clone(),
            state: state.clone(),
            exit_code: exit_code.clone(),
            rev: rev.clone(),
            ended_at: ended_at.clone(),
            wait_finished: wait_finished.clone(),
        }) {
            delete_failed_history(&history, &meta.id);
            running.store(false, Ordering::SeqCst);
            wait_finished.store(true, Ordering::SeqCst);
            return Err(err);
        }

        let session = Arc::new(Self {
            id: meta.id.clone(),
            project: req.project,
            cwd: req.cwd,
            agent: req.agent,
            terminal_id: req.terminal_id,
            output_tx: output_tx.clone(),
            output_buf: output_buf.clone(),
            state: state.clone(),
            exit_code: exit_code.clone(),
            rev: rev.clone(),
            created_at,
            ended_at: ended_at.clone(),
            live: Some(LiveSession {
                input_tx,
                master: std::sync::Mutex::new(pair.master),
                killer: std::sync::Mutex::new(live_killer),
                running: running.clone(),
            }),
            history: history.clone(),
        });

        Ok(session)
    }

    pub fn restored(meta: TerminalHistoryMeta, history: Arc<TerminalHistoryStore>) -> Arc<Self> {
        let restored_state = if meta.ended_at.is_some() || meta.state == SessionState::Exited {
            SessionState::Exited
        } else {
            SessionState::Restored
        };
        let (output_tx, _) = broadcast::channel(16);
        Arc::new(Self {
            id: meta.id,
            project: meta.project,
            cwd: meta.cwd,
            agent: meta.agent,
            terminal_id: meta.terminal_id,
            output_tx,
            output_buf: Arc::new(std::sync::Mutex::new(Vec::new())),
            state: Arc::new(std::sync::Mutex::new(restored_state)),
            exit_code: Arc::new(std::sync::Mutex::new(meta.exit_code)),
            rev: Arc::new(AtomicU64::new(2)),
            created_at: meta.created_at,
            ended_at: Arc::new(std::sync::Mutex::new(meta.ended_at)),
            live: None,
            history,
        })
    }

    pub async fn write_input(&self, data: &[u8]) -> anyhow::Result<()> {
        if data.len() > MAX_INPUT_BYTES {
            anyhow::bail!("input payload too large (>{MAX_INPUT_BYTES} bytes)");
        }
        let Some(live) = &self.live else {
            anyhow::bail!("terminal session is not live");
        };
        for chunk in data.chunks(INPUT_CHUNK_SIZE) {
            if !live.running.load(Ordering::SeqCst) {
                anyhow::bail!("terminal session has exited");
            }
            live.input_tx
                .send(chunk.to_vec())
                .await
                .map_err(|_| anyhow::anyhow!("terminal input queue closed"))?;
        }
        Ok(())
    }

    pub fn resize(&self, size: TerminalSize) -> anyhow::Result<()> {
        let Some(live) = &self.live else {
            anyhow::bail!("terminal session is not live");
        };
        let master = live.master.lock().expect("master lock");
        master.resize(pty_size(size))?;
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TerminalEvent> {
        self.output_tx.subscribe()
    }

    pub fn snapshot(&self) -> Vec<u8> {
        self.output_buf.lock().expect("output_buf lock").clone()
    }

    pub fn kill(&self) -> anyhow::Result<()> {
        if let Some(live) = &self.live {
            let mut killer = live.killer.lock().expect("killer lock");
            killer.kill()?;
        }
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.live
            .as_ref()
            .is_some_and(|live| live.running.load(Ordering::SeqCst))
    }

    pub fn state(&self) -> SessionState {
        self.state.lock().expect("state lock").clone()
    }

    pub fn exit_code(&self) -> Option<u32> {
        *self.exit_code.lock().expect("exit_code lock")
    }

    pub fn info(&self) -> SessionInfo {
        let state = self.state();
        SessionInfo {
            id: self.id.clone(),
            rev: self.rev.load(Ordering::SeqCst),
            project: self.project.clone(),
            cwd: self.cwd.clone(),
            agent: self.agent.clone(),
            running: self.is_running(),
            exit_code: self.exit_code(),
            restored: state == SessionState::Restored,
            state,
            created_at: self.created_at.clone(),
            ended_at: self.ended_at.lock().expect("ended_at lock").clone(),
            terminal_id: self.terminal_id.clone(),
        }
    }

    pub fn delete_history(&self) -> anyhow::Result<()> {
        self.history.delete_session(&self.id)
    }
}

fn delete_failed_history(history: &Arc<TerminalHistoryStore>, session_id: &str) {
    if let Err(err) = history.delete_session(session_id) {
        warn!(
            session = session_id,
            error = %err,
            "failed to delete terminal history after spawn error"
        );
    }
}

fn spawn_waiter_thread(ctx: WaiterThreadCtx) -> anyhow::Result<()> {
    let WaiterThreadCtx {
        session_id,
        mut child_guard,
        history,
        running,
        state,
        exit_code,
        rev,
        ended_at,
        wait_finished,
    } = ctx;
    std::thread::Builder::new()
        .name(format!("pty-wait-{session_id}"))
        .spawn(move || {
            let _ = child_guard.killer.take();
            let mut child = child_guard.child.take().expect("child guard missing child");
            let result = child.wait();
            let code = match result {
                Ok(status) => Some(status.exit_code()),
                Err(err) => {
                    warn!(session = session_id, error = %err, "terminal wait() failed");
                    None
                }
            };
            let ended = mando_types::now_rfc3339();
            *exit_code.lock().expect("exit_code lock") = code;
            *ended_at.lock().expect("ended_at lock") = Some(ended.clone());
            *state.lock().expect("state lock") = SessionState::Exited;
            rev.fetch_add(1, Ordering::SeqCst);
            running.store(false, Ordering::SeqCst);
            if let Err(err) = history.finish_session(&session_id, code, ended) {
                warn!(session = session_id, error = %err, "failed to finalize terminal history");
            }
            wait_finished.store(true, Ordering::SeqCst);
            debug!(session = session_id, code = ?code, "terminal process exited");
        })?;
    Ok(())
}
