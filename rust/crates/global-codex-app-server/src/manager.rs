use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result};
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;
use tokio::process::ChildStdin;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::time::timeout;

use crate::process::{cleanup_child_process, spawn_app_server, stderr_tail_context};
use crate::request_params;
use crate::types::{AppServerEvent, StartTurnRequest, StartedTurn, StderrTail};

mod events;
mod start_rollback;
use events::{
    broadcast_fatal_for_inner, fail_pending_for_inner, fail_pending_for_pid, reader_loop,
};
use start_rollback::rollback_started_thread;

#[derive(Clone)]
pub struct CodexAppServerManager {
    inner: Arc<Inner>,
}

struct Inner {
    state: Mutex<Option<ProcessState>>,
    start_lock: Mutex<()>,
    pending: Mutex<HashMap<u64, PendingRequest>>,
    subscribers: Mutex<HashMap<String, mpsc::UnboundedSender<AppServerEvent>>>,
    active_turns: Mutex<HashMap<String, ActiveTurn>>,
    interrupted_turns: Mutex<std::collections::HashSet<String>>,
    next_id: AtomicU64,
}

#[derive(Clone)]
struct ActiveTurn {
    turn_id: String,
    response_timeout: std::time::Duration,
}

struct PendingRequest {
    pid: global_types::Pid,
    method: String,
    params: Value,
    sender: oneshot::Sender<Result<Value>>,
}

struct ProcessState {
    stdin: ChildStdin,
    child: tokio::process::Child,
    pid: global_types::Pid,
    stderr_tail: StderrTail,
}

pub fn shared_manager() -> CodexAppServerManager {
    static MANAGER: OnceLock<CodexAppServerManager> = OnceLock::new();
    MANAGER.get_or_init(CodexAppServerManager::new).clone()
}

impl CodexAppServerManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                state: Mutex::new(None),
                start_lock: Mutex::new(()),
                pending: Mutex::new(HashMap::new()),
                subscribers: Mutex::new(HashMap::new()),
                active_turns: Mutex::new(HashMap::new()),
                interrupted_turns: Mutex::new(std::collections::HashSet::new()),
                next_id: AtomicU64::new(100),
            }),
        }
    }

    #[tracing::instrument(skip(self, request), fields(cwd = %request.cwd.display(), resume = request.resume_thread_id.is_some()))]
    pub async fn start_turn(&self, request: StartTurnRequest) -> Result<StartedTurn> {
        self.ensure_process(request.response_timeout).await?;
        let thread_params = request_params::thread_params(&request);
        let thread_method = if request.resume_thread_id.is_some() {
            "thread/resume"
        } else {
            "thread/start"
        };
        let thread_response = self
            .request(thread_method, thread_params, request.response_timeout)
            .await
            .with_context(|| format!("Codex app-server {thread_method}"))?;
        let thread_id = thread_response
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .context("thread start/resume response missing thread.id")?
            .to_string();
        let model = thread_response
            .get("model")
            .or_else(|| thread_response.pointer("/thread/model"))
            .and_then(Value::as_str)
            .unwrap_or("codex")
            .to_string();

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        self.inner
            .subscribers
            .lock()
            .await
            .insert(thread_id.clone(), event_tx);
        let expects_structured_output = request.output_schema.is_some();
        let turn_params = request_params::turn_params(&thread_id, &request);
        let turn_response = match self
            .request("turn/start", turn_params, request.response_timeout)
            .await
        {
            Ok(response) => response,
            Err(e) => {
                rollback_started_thread(
                    self,
                    &thread_id,
                    None,
                    request.response_timeout,
                    "turn/start failed",
                )
                .await;
                return Err(e).context("Codex app-server turn/start");
            }
        };
        let turn_id = match turn_response.pointer("/turn/id").and_then(Value::as_str) {
            Some(turn_id) => turn_id.to_string(),
            None => {
                rollback_started_thread(
                    self,
                    &thread_id,
                    None,
                    request.response_timeout,
                    "turn/start missing turn id",
                )
                .await;
                anyhow::bail!("turn/start response missing turn.id");
            }
        };
        self.inner.active_turns.lock().await.insert(
            thread_id.clone(),
            ActiveTurn {
                turn_id: turn_id.clone(),
                response_timeout: request.response_timeout,
            },
        );
        let (pid, stderr_tail) = match self.process_info().await {
            Some(process_info) => process_info,
            None => {
                rollback_started_thread(
                    self,
                    &thread_id,
                    Some(&turn_id),
                    request.response_timeout,
                    "process unavailable after turn/start",
                )
                .await;
                anyhow::bail!("Codex app-server process unavailable after turn/start");
            }
        };
        tracing::info!(
            module = "codex_app_server",
            thread_id,
            turn_id,
            pid = %pid,
            "started Codex turn on shared app-server"
        );
        Ok(StartedTurn {
            thread_id,
            turn_id,
            model,
            pid,
            stderr_tail,
            expects_structured_output,
            response_timeout: request.response_timeout,
            events: event_rx,
        })
    }

    #[tracing::instrument(skip(self, message), fields(thread_id))]
    pub async fn steer(&self, thread_id: &str, message: String) -> Result<bool> {
        let Some(active) = self.active_turn(thread_id).await else {
            return Ok(false);
        };
        self.request(
            "turn/steer",
            json!({
                "threadId": thread_id,
                "expectedTurnId": &active.turn_id,
                "input": [{"type": "text", "text": message}],
            }),
            active.response_timeout,
        )
        .await?;
        Ok(true)
    }

    #[tracing::instrument(skip(self), fields(thread_id))]
    pub async fn interrupt(&self, thread_id: &str) -> Result<bool> {
        let Some(active) = self.active_turn(thread_id).await else {
            return Ok(false);
        };
        self.request(
            "turn/interrupt",
            json!({"threadId": thread_id, "turnId": &active.turn_id}),
            active.response_timeout,
        )
        .await?;
        self.inner
            .interrupted_turns
            .lock()
            .await
            .insert(thread_id.to_string());
        Ok(true)
    }

    #[tracing::instrument(skip(self), fields(thread_id))]
    pub async fn unsubscribe(
        &self,
        thread_id: &str,
        response_timeout: std::time::Duration,
    ) -> Result<()> {
        self.unsubscribe_local(thread_id).await;
        if self.process_info().await.is_none() {
            return Ok(());
        }
        self.request(
            "thread/unsubscribe",
            json!({"threadId": thread_id}),
            response_timeout,
        )
        .await
        .map(|_| ())
    }

    pub async fn is_turn_active(&self, thread_id: &str) -> bool {
        self.inner.active_turns.lock().await.contains_key(thread_id)
    }

    pub async fn active_turn_id(&self, thread_id: &str) -> Option<String> {
        self.active_turn(thread_id)
            .await
            .map(|active| active.turn_id)
    }

    pub async fn take_interrupted(&self, thread_id: &str) -> bool {
        self.inner.interrupted_turns.lock().await.remove(thread_id)
    }

    pub async fn cleanup_thread(&self, thread_id: &str) {
        self.inner.active_turns.lock().await.remove(thread_id);
        self.unsubscribe_local(thread_id).await;
        self.inner.interrupted_turns.lock().await.remove(thread_id);
    }

    async fn ensure_process(&self, response_timeout: std::time::Duration) -> Result<()> {
        let _guard = self.inner.start_lock.lock().await;
        if self.process_info().await.is_some() {
            return Ok(());
        }
        if let Some(stale) = self.take_unhealthy_process().await {
            let pid = stale.pid;
            fail_pending_for_pid(&self.inner, pid, "Codex app-server process closed").await;
            self.broadcast_fatal("Codex app-server process closed")
                .await;
            cleanup_child_process(stale.child, "replacing unhealthy app-server").await;
        }
        let spawned = spawn_app_server()?;
        let pid = spawned.pid;
        let stderr_tail = spawned.stderr_tail.clone();
        let stdout = spawned.stdout;
        {
            let mut state = self.inner.state.lock().await;
            *state = Some(ProcessState {
                stdin: spawned.stdin,
                child: spawned.child,
                pid,
                stderr_tail: stderr_tail.clone(),
            });
        }
        tokio::spawn(reader_loop(self.inner.clone(), stdout, pid));
        let init_result = self
            .request(
                "initialize",
                json!({
                    "clientInfo": {"name": "mando", "version": env!("CARGO_PKG_VERSION")},
                    "capabilities": {"experimentalApi": true},
                }),
                response_timeout,
            )
            .await
            .with_context(|| {
                format!(
                    "initialize Codex app-server{}",
                    stderr_tail_context(&stderr_tail)
                )
            });
        if let Err(e) = init_result {
            self.reset_process("initialize failed").await;
            return Err(e);
        }
        if let Err(e) = self.notification("initialized", json!({})).await {
            self.reset_process("initialized notification failed").await;
            return Err(e);
        }
        tracing::info!(module = "codex_app_server", pid = %pid, "initialized shared Codex app-server");
        Ok(())
    }

    async fn request(
        &self,
        method: &str,
        params: Value,
        response_timeout: std::time::Duration,
    ) -> Result<Value> {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        let pid = self
            .process_pid()
            .await
            .context("Codex app-server is not running")?;
        let pending_params = params.clone();
        self.inner.pending.lock().await.insert(
            id,
            PendingRequest {
                pid,
                method: method.to_string(),
                params: pending_params,
                sender: tx,
            },
        );
        if let Err(e) = self
            .write_message_to_pid(
                pid,
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "method": method,
                    "params": params,
                }),
            )
            .await
        {
            self.inner.pending.lock().await.remove(&id);
            self.reset_process_if_current(pid, "request write failed")
                .await;
            return Err(e).with_context(|| format!("write Codex app-server request {method}"));
        }
        match timeout(response_timeout, rx).await {
            Ok(Ok(response)) => response.with_context(|| format!("Codex app-server {method}")),
            Ok(Err(_)) => anyhow::bail!("Codex app-server response sender dropped for {method}"),
            Err(_) => {
                let pending = self.inner.pending.lock().await.remove(&id);
                if let Some(pending) = pending {
                    self.reset_process_if_current(pending.pid, "request timed out")
                        .await;
                }
                anyhow::bail!(
                    "timed out waiting for Codex app-server response to {method} after {}s",
                    response_timeout.as_secs()
                )
            }
        }
    }

    async fn notification(&self, method: &str, params: Value) -> Result<()> {
        self.write_message(json!({"jsonrpc": "2.0", "method": method, "params": params}))
            .await
            .with_context(|| format!("write Codex app-server notification {method}"))
    }

    async fn write_message(&self, value: Value) -> Result<()> {
        let mut state = self.inner.state.lock().await;
        let process = state.as_mut().context("Codex app-server is not running")?;
        write_message_to_process(process, value).await
    }

    async fn write_message_to_pid(&self, pid: global_types::Pid, value: Value) -> Result<()> {
        let mut state = self.inner.state.lock().await;
        let process = state.as_mut().context("Codex app-server is not running")?;
        if process.pid != pid {
            anyhow::bail!("Codex app-server process changed before request write");
        }
        write_message_to_process(process, value).await
    }

    async fn reset_process_if_current(&self, pid: global_types::Pid, reason: &'static str) {
        let child = {
            let mut state = self.inner.state.lock().await;
            match state.as_ref() {
                Some(process) if process.pid == pid => state.take().map(|process| process.child),
                _ => None,
            }
        };
        let Some(child) = child else {
            tracing::debug!(
                module = "codex_app_server",
                pid = %pid,
                "not resetting Codex app-server because request belonged to an old process"
            );
            return;
        };
        fail_pending_for_pid(&self.inner, pid, "Codex app-server process reset").await;
        self.broadcast_fatal("Codex app-server process reset").await;
        cleanup_child_process(child, reason).await;
    }

    async fn reset_process(&self, reason: &'static str) {
        let child = {
            let mut state = self.inner.state.lock().await;
            state.take().map(|process| process.child)
        };
        self.fail_pending("Codex app-server process reset").await;
        self.broadcast_fatal("Codex app-server process reset").await;
        if let Some(child) = child {
            cleanup_child_process(child, reason).await;
        }
    }

    async fn process_info(&self) -> Option<(global_types::Pid, StderrTail)> {
        let mut state = self.inner.state.lock().await;
        let process = state.as_mut()?;
        if process.pid.as_u32() == 0 || !global_claude::is_process_alive(process.pid) {
            return None;
        }
        Some((process.pid, process.stderr_tail.clone()))
    }

    async fn process_pid(&self) -> Option<global_types::Pid> {
        let state = self.inner.state.lock().await;
        state.as_ref().map(|process| process.pid)
    }

    async fn take_unhealthy_process(&self) -> Option<ProcessState> {
        let mut state = self.inner.state.lock().await;
        match state.as_ref() {
            Some(process)
                if process.pid.as_u32() == 0 || !global_claude::is_process_alive(process.pid) =>
            {
                state.take()
            }
            _ => None,
        }
    }

    async fn active_turn(&self, thread_id: &str) -> Option<ActiveTurn> {
        self.inner.active_turns.lock().await.get(thread_id).cloned()
    }

    async fn unsubscribe_local(&self, thread_id: &str) {
        self.inner.subscribers.lock().await.remove(thread_id);
    }
}

async fn write_message_to_process(process: &mut ProcessState, value: Value) -> Result<()> {
    let line = serde_json::to_string(&value)?;
    process.stdin.write_all(line.as_bytes()).await?;
    process.stdin.write_all(b"\n").await?;
    process.stdin.flush().await?;
    Ok(())
}

impl CodexAppServerManager {
    async fn fail_pending(&self, reason: &'static str) {
        fail_pending_for_inner(&self.inner, reason).await;
    }

    async fn broadcast_fatal(&self, reason: &'static str) {
        broadcast_fatal_for_inner(&self.inner, reason).await;
    }
}
