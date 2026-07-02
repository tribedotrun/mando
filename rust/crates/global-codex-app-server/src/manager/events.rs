use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::Context;
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex as AsyncMutex;

use crate::process::cleanup_child_process;
use crate::types::AppServerEvent;

use super::{write_message_to_process, Inner};

const COMMAND_APPROVAL_REQUEST: &str = "item/commandExecution/requestApproval";
const FILE_CHANGE_APPROVAL_REQUEST: &str = "item/fileChange/requestApproval";
const MCP_ELICITATION_REQUEST: &str = "mcpServer/elicitation/request";
const PERMISSIONS_APPROVAL_REQUEST: &str = "item/permissions/requestApproval";
const TOOL_USER_INPUT_REQUEST: &str = "item/tool/requestUserInput";
const MCP_APPROVAL_KIND_KEY: &str = "codex_approval_kind";
const MCP_APPROVAL_KIND_TOOL_CALL: &str = "mcp_tool_call";
const MCP_APPROVAL_PERSIST_KEY: &str = "persist";
const MCP_APPROVAL_PERSIST_SESSION: &str = "session";
const MCP_APPROVAL_PERSIST_ALWAYS: &str = "always";
const APPROVAL_ALLOW: &str = "Allow";
const APPROVAL_ALLOW_FOR_SESSION: &str = "Allow for this session";
const APPROVAL_ALLOW_ALWAYS: &str = "Allow and don't ask me again";

pub(super) async fn reader_loop(
    inner: Arc<Inner>,
    mut stdout: tokio::io::Lines<tokio::io::BufReader<tokio::process::ChildStdout>>,
    pid: global_types::Pid,
) {
    loop {
        match stdout.next_line().await {
            Ok(Some(line)) => match serde_json::from_str::<Value>(&line) {
                Ok(value) => route_message(&inner, pid, value, &line).await,
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

async fn route_message(inner: &Arc<Inner>, pid: global_types::Pid, value: Value, raw_line: &str) {
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if let Some(id) = value.get("id").cloned() {
        if is_server_request(&value) {
            route_server_request(inner, pid, id, &method, value, raw_line).await;
            return;
        }
        if let Some(id) = id.as_u64() {
            route_response(inner, id, value, raw_line).await;
            return;
        }
        tracing::warn!(
            module = "codex_app_server",
            request_id = %request_id_label(&id),
            "Codex app-server response used a non-integer id Mando cannot match"
        );
        return;
    }
    route_notification(inner, value, raw_line, &method).await;
}

fn is_server_request(value: &Value) -> bool {
    value.get("id").is_some() && value.get("method").and_then(Value::as_str).is_some()
}

async fn route_notification(inner: &Arc<Inner>, value: Value, raw_line: &str, method: &str) {
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

async fn route_server_request(
    inner: &Arc<Inner>,
    pid: global_types::Pid,
    id: Value,
    method: &str,
    value: Value,
    raw_line: &str,
) {
    let thread_id = crate::routing::notification_thread_id(&value);
    if let Some(thread_id) = &thread_id {
        if let Err(e) = append_codex_jsonl(thread_id, raw_line).await {
            let message = format!("failed to persist Codex app-server server request: {e}");
            tracing::warn!(
                module = "codex_app_server",
                thread_id,
                request_id = %request_id_label(&id),
                method,
                error = %e,
                "failed to append Codex app-server server request"
            );
            let stale = match inner.subscribers.lock().await.get(thread_id).cloned() {
                Some(tx) => tx.send(AppServerEvent::Fatal(message)).is_err(),
                None => false,
            };
            if stale {
                inner.subscribers.lock().await.remove(thread_id);
            }
        }
    }

    let response =
        auto_response_for_server_request(method, value.get("params").unwrap_or(&Value::Null));
    let decision = response.decision_label();
    tracing::info!(
        module = "codex_app_server",
        thread_id = thread_id.as_deref().unwrap_or(""),
        request_id = %request_id_label(&id),
        method,
        decision,
        "handled Codex app-server server request"
    );

    let message = match response {
        ServerRequestResponse::Result(result) => {
            json!({"jsonrpc": "2.0", "id": id, "result": result})
        }
        ServerRequestResponse::Error(message) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32603,
                "message": message,
            },
        }),
    };
    if let Err(e) = write_server_response(inner, pid, message).await {
        tracing::warn!(
            module = "codex_app_server",
            thread_id = thread_id.as_deref().unwrap_or(""),
            method,
            error = %e,
            "failed to send Codex app-server server-request response"
        );
    }
}

enum ServerRequestResponse {
    Result(Value),
    Error(String),
}

impl ServerRequestResponse {
    fn decision_label(&self) -> &'static str {
        match self {
            Self::Result(_) => "auto_approved",
            Self::Error(_) => "unsupported_rejected",
        }
    }
}

fn auto_response_for_server_request(method: &str, params: &Value) -> ServerRequestResponse {
    match method {
        COMMAND_APPROVAL_REQUEST => ServerRequestResponse::Result(json!({
            "decision": command_approval_decision(params),
        })),
        FILE_CHANGE_APPROVAL_REQUEST => ServerRequestResponse::Result(json!({
            "decision": file_change_approval_decision(params),
        })),
        MCP_ELICITATION_REQUEST => mcp_elicitation_response(params),
        PERMISSIONS_APPROVAL_REQUEST => ServerRequestResponse::Result(json!({
            "permissions": params.get("permissions").cloned().unwrap_or_else(|| json!({})),
            "scope": "session",
            "strictAutoReview": false,
        })),
        TOOL_USER_INPUT_REQUEST => tool_user_input_response(params),
        _ => ServerRequestResponse::Error(format!(
            "Mando does not support Codex app-server server request {method}"
        )),
    }
}

fn mcp_elicitation_response(params: &Value) -> ServerRequestResponse {
    if !is_mcp_tool_approval_elicitation(params) {
        return ServerRequestResponse::Error(
            "Codex MCP elicitation was not an MCP tool approval prompt".into(),
        );
    }

    let mut result = serde_json::Map::new();
    result.insert("action".into(), json!("accept"));
    result.insert("content".into(), Value::Null);
    if let Some(persist) = mcp_elicitation_persist_choice(params) {
        result.insert("_meta".into(), json!({ MCP_APPROVAL_PERSIST_KEY: persist }));
    }
    ServerRequestResponse::Result(Value::Object(result))
}

fn is_mcp_tool_approval_elicitation(params: &Value) -> bool {
    params
        .get("_meta")
        .and_then(Value::as_object)
        .and_then(|meta| meta.get(MCP_APPROVAL_KIND_KEY))
        .and_then(Value::as_str)
        == Some(MCP_APPROVAL_KIND_TOOL_CALL)
}

fn mcp_elicitation_persist_choice(params: &Value) -> Option<&'static str> {
    let persist = params
        .get("_meta")
        .and_then(Value::as_object)
        .and_then(|meta| meta.get(MCP_APPROVAL_PERSIST_KEY))?;
    if persist_value_includes(persist, MCP_APPROVAL_PERSIST_SESSION) {
        Some(MCP_APPROVAL_PERSIST_SESSION)
    } else if persist_value_includes(persist, MCP_APPROVAL_PERSIST_ALWAYS) {
        Some(MCP_APPROVAL_PERSIST_ALWAYS)
    } else {
        None
    }
}

fn persist_value_includes(value: &Value, expected: &str) -> bool {
    value.as_str() == Some(expected)
        || value
            .as_array()
            .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(expected)))
}

fn command_approval_decision(params: &Value) -> &'static str {
    if approval_decision_available(params, "acceptForSession") {
        "acceptForSession"
    } else {
        "accept"
    }
}

fn file_change_approval_decision(params: &Value) -> &'static str {
    if approval_decision_available(params, "acceptForSession") {
        "acceptForSession"
    } else {
        "accept"
    }
}

fn approval_decision_available(params: &Value, decision: &str) -> bool {
    params
        .get("availableDecisions")
        .and_then(Value::as_array)
        .is_some_and(|decisions| {
            decisions
                .iter()
                .any(|value| value.as_str() == Some(decision))
        })
}

fn tool_user_input_response(params: &Value) -> ServerRequestResponse {
    let Some(questions) = params.get("questions").and_then(Value::as_array) else {
        return ServerRequestResponse::Error(
            "Codex tool user-input request did not include questions".into(),
        );
    };
    let mut answers = serde_json::Map::new();
    for question in questions {
        let Some(question_id) = question.get("id").and_then(Value::as_str) else {
            return ServerRequestResponse::Error(
                "Codex tool user-input question did not include id".into(),
            );
        };
        let Some(answer) = approval_answer_for_question(question) else {
            return ServerRequestResponse::Error(format!(
                "Codex tool user-input question {question_id} was not an approval prompt"
            ));
        };
        answers.insert(question_id.to_string(), json!({"answers": [answer]}));
    }
    ServerRequestResponse::Result(json!({ "answers": answers }))
}

fn approval_answer_for_question(question: &Value) -> Option<&'static str> {
    let options = question.get("options").and_then(Value::as_array)?;
    if option_label_available(options, APPROVAL_ALLOW_FOR_SESSION) {
        Some(APPROVAL_ALLOW_FOR_SESSION)
    } else if option_label_available(options, APPROVAL_ALLOW) {
        Some(APPROVAL_ALLOW)
    } else if option_label_available(options, APPROVAL_ALLOW_ALWAYS) {
        Some(APPROVAL_ALLOW_ALWAYS)
    } else {
        None
    }
}

fn option_label_available(options: &[Value], label: &str) -> bool {
    options
        .iter()
        .any(|option| option.get("label").and_then(Value::as_str) == Some(label))
}

async fn write_server_response(
    inner: &Arc<Inner>,
    pid: global_types::Pid,
    value: Value,
) -> anyhow::Result<()> {
    let mut state = inner.state.lock().await;
    let process = state.as_mut().context("Codex app-server is not running")?;
    if process.pid != pid {
        tracing::debug!(
            module = "codex_app_server",
            request_pid = %pid,
            current_pid = %process.pid,
            "ignoring stale Codex app-server server-request response"
        );
        return Ok(());
    }
    write_message_to_process(process, value).await
}

fn request_id_label(id: &Value) -> String {
    id.as_str()
        .map(str::to_string)
        .unwrap_or_else(|| id.to_string())
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

    #[test]
    fn id_and_method_message_is_server_request() {
        let value = serde_json::json!({
            "id": 12,
            "method": MCP_ELICITATION_REQUEST,
            "params": {"threadId": "thread", "serverName": "computer-use"}
        });

        assert!(is_server_request(&value));
    }

    #[test]
    fn id_without_method_message_is_response() {
        let value = serde_json::json!({"id": 12, "result": {"turnId": "turn"}});

        assert!(!is_server_request(&value));
    }

    #[test]
    fn mcp_elicitation_auto_response_accepts_for_session() {
        let response = auto_response_for_server_request(
            MCP_ELICITATION_REQUEST,
            &serde_json::json!({
                "threadId": "thread",
                "serverName": "computer-use",
                "mode": "form",
                "_meta": {
                    "codex_approval_kind": "mcp_tool_call",
                    "persist": ["session", "always"]
                }
            }),
        );

        match response {
            ServerRequestResponse::Result(result) => {
                assert_eq!(result.get("action"), Some(&serde_json::json!("accept")));
                assert_eq!(
                    result.pointer("/_meta/persist"),
                    Some(&serde_json::json!("session"))
                );
            }
            ServerRequestResponse::Error(message) => panic!("unexpected error: {message}"),
        }
    }

    #[test]
    fn mcp_elicitation_auto_response_rejects_non_approval_prompt() {
        let response = auto_response_for_server_request(
            MCP_ELICITATION_REQUEST,
            &serde_json::json!({
                "threadId": "thread",
                "serverName": "computer-use",
                "mode": "form",
                "_meta": {"codex_request_type": "structured_input"}
            }),
        );

        match response {
            ServerRequestResponse::Result(result) => panic!("unexpected result: {result}"),
            ServerRequestResponse::Error(message) => {
                assert!(message.contains("not an MCP tool approval prompt"));
            }
        }
    }

    #[test]
    fn tool_user_input_auto_response_picks_session_approval() {
        let response = auto_response_for_server_request(
            TOOL_USER_INPUT_REQUEST,
            &serde_json::json!({
                "threadId": "thread",
                "turnId": "turn",
                "itemId": "call-1",
                "questions": [{
                    "id": "mcp_tool_call_approval_call-1",
                    "header": "Computer Use",
                    "question": "Allow tool?",
                    "options": [
                        {"label": "Allow", "description": "Run once."},
                        {"label": "Allow for this session", "description": "Remember."}
                    ]
                }]
            }),
        );

        match response {
            ServerRequestResponse::Result(result) => assert_eq!(
                result.pointer("/answers/mcp_tool_call_approval_call-1/answers/0"),
                Some(&serde_json::json!(APPROVAL_ALLOW_FOR_SESSION))
            ),
            ServerRequestResponse::Error(message) => panic!("unexpected error: {message}"),
        }
    }

    #[test]
    fn command_approval_auto_response_uses_session_when_available() {
        let response = auto_response_for_server_request(
            COMMAND_APPROVAL_REQUEST,
            &serde_json::json!({
                "threadId": "thread",
                "turnId": "turn",
                "itemId": "cmd-1",
                "availableDecisions": ["accept", "acceptForSession", "decline"]
            }),
        );

        match response {
            ServerRequestResponse::Result(result) => assert_eq!(
                result.get("decision"),
                Some(&serde_json::json!("acceptForSession"))
            ),
            ServerRequestResponse::Error(message) => panic!("unexpected error: {message}"),
        }
    }

    #[test]
    fn permissions_auto_response_grants_requested_permissions_for_session() {
        let response = auto_response_for_server_request(
            PERMISSIONS_APPROVAL_REQUEST,
            &serde_json::json!({
                "threadId": "thread",
                "turnId": "turn",
                "itemId": "permissions-1",
                "permissions": {"network": {"enabled": true}}
            }),
        );

        match response {
            ServerRequestResponse::Result(result) => {
                assert_eq!(
                    result.pointer("/permissions/network/enabled"),
                    Some(&serde_json::json!(true))
                );
                assert_eq!(result.get("scope"), Some(&serde_json::json!("session")));
                assert_eq!(
                    result.get("strictAutoReview"),
                    Some(&serde_json::json!(false))
                );
            }
            ServerRequestResponse::Error(message) => panic!("unexpected error: {message}"),
        }
    }

    #[test]
    fn unsupported_server_request_returns_error_response() {
        let response = auto_response_for_server_request(
            "item/tool/requestUserInput",
            &serde_json::json!({
                "questions": [{
                    "id": "name",
                    "header": "Name",
                    "question": "What name?",
                    "options": [{"label": "Bill", "description": "Use this name."}]
                }]
            }),
        );

        match response {
            ServerRequestResponse::Result(result) => panic!("unexpected result: {result}"),
            ServerRequestResponse::Error(message) => {
                assert!(message.contains("was not an approval prompt"));
            }
        }
    }

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
