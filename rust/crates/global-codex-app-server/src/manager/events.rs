use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::Context;
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex as AsyncMutex;

use crate::process::cleanup_child_process;
use crate::types::AppServerEvent;

use super::Inner;

pub(super) async fn reader_loop(
    inner: Arc<Inner>,
    mut stdout: tokio::io::Lines<tokio::io::BufReader<tokio::process::ChildStdout>>,
    pid: global_types::Pid,
) {
    loop {
        match stdout.next_line().await {
            Ok(Some(line)) => match serde_json::from_str::<Value>(&line) {
                Ok(value) => route_message(&inner, value, &line).await,
                Err(e) => {
                    tracing::warn!(module = "codex_app_server", pid = %pid, error = %e, "invalid app-server JSON line")
                }
            },
            Ok(None) => {
                mark_process_closed(&inner, pid, "stdout closed").await;
                break;
            }
            Err(e) => {
                tracing::warn!(module = "codex_app_server", pid = %pid, error = %e, "failed to read Codex stdout");
                mark_process_closed(&inner, pid, "stdout read failed").await;
                break;
            }
        }
    }
}

async fn route_message(inner: &Arc<Inner>, value: Value, raw_line: &str) {
    if let Some(id) = value.get("id").and_then(Value::as_u64) {
        route_response(inner, id, value, raw_line).await;
        return;
    }
    let method = value.get("method").and_then(Value::as_str).unwrap_or("");
    let thread_id = crate::routing::notification_thread_id(&value);
    if method == "turn/completed" {
        if let Some(thread_id) = &thread_id {
            inner.active_turns.lock().await.remove(thread_id);
        }
    }
    if let Some(thread_id) = thread_id {
        if let Err(e) = append_codex_jsonl(&thread_id, raw_line).await {
            let message = format!("failed to persist Codex app-server JSONL: {e}");
            tracing::warn!(
                module = "codex_app_server",
                thread_id,
                error = %e,
                "failed to append raw Codex app-server JSONL"
            );
            let stale = match inner.subscribers.lock().await.get(&thread_id).cloned() {
                Some(tx) => tx.send(AppServerEvent::Fatal(message)).is_err(),
                None => false,
            };
            if stale {
                inner.subscribers.lock().await.remove(&thread_id);
            }
            return;
        }
        let stale = match inner.subscribers.lock().await.get(&thread_id).cloned() {
            Some(tx) => tx.send(AppServerEvent::Notification(value)).is_err(),
            None => false,
        };
        if stale {
            inner.subscribers.lock().await.remove(&thread_id);
        }
    }
}

async fn route_response(inner: &Arc<Inner>, id: u64, value: Value, raw_line: &str) {
    let pending = inner.pending.lock().await.remove(&id);
    let Some(pending) = pending else {
        tracing::debug!(
            module = "codex_app_server",
            request_id = id,
            "ignoring untracked Codex app-server response"
        );
        return;
    };
    if let Some(thread_id) =
        crate::routing::response_thread_id(&pending.method, &pending.params, &value)
    {
        if let Err(e) = append_codex_jsonl(&thread_id, raw_line).await {
            drop(pending.sender.send(Err(
                e.context("persist Codex app-server response before delivering request result"),
            )));
            return;
        }
    }
    let result = if let Some(error) = value.get("error") {
        Err(anyhow::anyhow!(
            "Codex app-server request {id} failed: {error}"
        ))
    } else {
        value
            .get("result")
            .cloned()
            .context("Codex app-server response missing result")
    };
    if pending.sender.send(result).is_err() {
        tracing::debug!(
            module = "codex_app_server",
            request_id = id,
            "Codex response receiver dropped"
        );
    }
}

async fn append_codex_jsonl(thread_id: &str, raw_line: &str) -> anyhow::Result<()> {
    let path = global_infra::paths::codex_session_jsonl_path(thread_id);
    let stream_lock = stream_lock(&path);
    let _guard = stream_lock.lock().await;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.with_context(|| {
            format!("create Codex session JSONL directory {}", parent.display())
        })?;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await
        .with_context(|| format!("open Codex session JSONL {}", path.display()))?;
    file.write_all(format!("{raw_line}\n").as_bytes())
        .await
        .with_context(|| format!("append Codex session JSONL {}", path.display()))?;
    Ok(())
}

fn stream_lock(path: &Path) -> Arc<AsyncMutex<()>> {
    static STREAM_LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<AsyncMutex<()>>>>> = OnceLock::new();
    let locks = STREAM_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    match locks.lock() {
        Ok(mut locks) => locks
            .entry(path.to_path_buf())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone(),
        Err(_) => {
            tracing::warn!(
                module = "codex_app_server",
                path = %path.display(),
                "stream lock map poisoned; using one-off lock"
            );
            Arc::new(AsyncMutex::new(()))
        }
    }
}

async fn mark_process_closed(inner: &Arc<Inner>, pid: global_types::Pid, reason: &'static str) {
    let child = {
        let mut state = inner.state.lock().await;
        match state.as_ref() {
            Some(process) if process.pid == pid => state.take().map(|process| process.child),
            _ => None,
        }
    };
    let Some(child) = child else {
        fail_pending_for_pid(inner, pid, "Codex app-server process closed").await;
        tracing::debug!(
            module = "codex_app_server",
            pid = %pid,
            "ignoring close from stale Codex app-server process"
        );
        return;
    };
    fail_pending_for_pid(inner, pid, "Codex app-server process closed").await;
    broadcast_fatal_for_inner(inner, "Codex app-server process closed").await;
    cleanup_child_process(child, reason).await;
}

pub(super) async fn fail_pending_for_inner(inner: &Arc<Inner>, reason: &'static str) {
    let pending = std::mem::take(&mut *inner.pending.lock().await);
    for (_id, pending) in pending {
        drop(pending.sender.send(Err(anyhow::anyhow!(reason))));
    }
}

pub(super) async fn fail_pending_for_pid(
    inner: &Arc<Inner>,
    pid: global_types::Pid,
    reason: &'static str,
) {
    let mut pending = inner.pending.lock().await;
    let ids = pending
        .iter()
        .filter_map(|(id, request)| (request.pid == pid).then_some(*id))
        .collect::<Vec<_>>();
    for id in ids {
        if let Some(request) = pending.remove(&id) {
            drop(request.sender.send(Err(anyhow::anyhow!(reason))));
        }
    }
}

pub(super) async fn broadcast_fatal_for_inner(inner: &Arc<Inner>, reason: &'static str) {
    inner.active_turns.lock().await.clear();
    let subscribers = std::mem::take(&mut *inner.subscribers.lock().await);
    for (_thread_id, sender) in subscribers {
        drop(sender.send(AppServerEvent::Fatal(reason.to_string())));
    }
}

#[cfg(test)]
mod tests {
    use super::super::{ActiveTurn, CodexAppServerManager, PendingRequest};
    use super::*;
    use tokio::sync::{mpsc, oneshot};

    #[tokio::test]
    async fn stale_process_close_keeps_current_state_intact() {
        let manager = CodexAppServerManager::new();
        let inner = manager.inner;
        let stale_pid = global_types::Pid::new(123);
        let current_pid = global_types::Pid::new(456);
        let (stale_tx, stale_rx) = oneshot::channel();
        let (current_tx, _current_rx) = oneshot::channel();
        inner.pending.lock().await.insert(
            1,
            PendingRequest {
                pid: stale_pid,
                method: "turn/start".into(),
                params: serde_json::json!({"threadId": "stale"}),
                sender: stale_tx,
            },
        );
        inner.pending.lock().await.insert(
            2,
            PendingRequest {
                pid: current_pid,
                method: "turn/start".into(),
                params: serde_json::json!({"threadId": "thread"}),
                sender: current_tx,
            },
        );
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        inner
            .subscribers
            .lock()
            .await
            .insert("thread".into(), event_tx);
        inner.active_turns.lock().await.insert(
            "thread".into(),
            ActiveTurn {
                turn_id: "turn".into(),
                response_timeout: std::time::Duration::from_secs(1),
            },
        );

        mark_process_closed(&inner, stale_pid, "test stale close").await;

        let pending = inner.pending.lock().await;
        assert!(!pending.contains_key(&1));
        assert!(pending.contains_key(&2));
        drop(pending);
        assert!(inner.subscribers.lock().await.contains_key("thread"));
        assert!(inner.active_turns.lock().await.contains_key("thread"));
        assert!(event_rx.try_recv().is_err());
        let stale_result = stale_rx.await.expect("stale pending request failed");
        assert!(stale_result.is_err());
    }
}
