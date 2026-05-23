use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result};
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex as AsyncMutex;

pub(super) struct CodexNotification<'a>(pub(super) &'a Value);

pub(super) struct CodexStreamLine(pub(super) Value);

pub(super) struct CodexStructuredOutput(pub(super) Value);

#[derive(Default)]
pub(super) struct CodexStreamState {
    last_usage: Option<Value>,
    duration_ms: Option<i64>,
    last_error: Option<Value>,
    agent_message_delta_items: HashSet<String>,
}

impl CodexStreamState {
    pub(super) fn duration_ms(&self) -> Option<i64> {
        self.duration_ms
    }

    pub(super) fn estimated_cost_usd(&self, model: &str) -> Option<f64> {
        let usage = normalize_usage(self.last_usage.as_ref())?;
        let input = usage
            .get("input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let output = usage
            .get("output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let cache_read = usage
            .get("cache_read_input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let cache_creation = usage
            .get("cache_creation_input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let cost = global_claude::rate_for_model(model).cost_for(
            input,
            output,
            cache_creation,
            cache_read,
        );
        (cost > 0.0).then_some(cost)
    }

    pub(super) fn failure_text(&self) -> Option<String> {
        self.last_error.as_ref().map(error_text)
    }
}

#[tracing::instrument(skip(stream_path, notification, state))]
pub(super) async fn handle_notification(
    stream_path: &Path,
    notification: CodexNotification<'_>,
    state: &mut CodexStreamState,
) -> bool {
    let value = notification.0;
    let method = value.get("method").and_then(Value::as_str).unwrap_or("");
    let params = value.get("params").unwrap_or(&Value::Null);
    match method {
        "item/agentMessage/delta" => {
            let text = params.get("delta").and_then(Value::as_str).unwrap_or("");
            state.agent_message_delta_items.insert(item_key(params));
            log_append_error(append_assistant_text(stream_path, text).await);
        }
        "item/completed" => {
            log_append_error(append_item_completed(stream_path, params, state).await)
        }
        "item/started" => log_append_error(append_item_started(stream_path, params).await),
        "item/commandExecution/outputDelta"
        | "item/fileChange/outputDelta"
        | "item/commandExecution/output/delta"
        | "item/fileChange/output/delta" => {
            let text = params.get("delta").and_then(Value::as_str).unwrap_or("");
            log_append_error(append_tool_result(stream_path, text).await);
        }
        "thread/tokenUsage/updated" => {
            state.last_usage = params.get("tokenUsage").cloned();
        }
        "error" => {
            state.last_error = Some(params.clone());
            log_append_error(
                append_tool_result(
                    stream_path,
                    &format!("Codex app-server error: {}", error_text(params)),
                )
                .await,
            );
        }
        "turn/completed" => {
            state.duration_ms = params.pointer("/turn/durationMs").and_then(Value::as_i64);
            if let Some(error) = params
                .pointer("/turn/error")
                .filter(|value| !value.is_null())
            {
                state.last_error = Some(error.clone());
                log_append_error(
                    append_tool_result(
                        stream_path,
                        &format!("Codex turn error: {}", error_text(error)),
                    )
                    .await,
                );
            }
            return true;
        }
        _ => {}
    }
    false
}

pub(super) fn turn_status(notification: CodexNotification<'_>) -> global_types::SessionStatus {
    match notification
        .0
        .pointer("/params/turn/status")
        .and_then(Value::as_str)
    {
        Some("completed" | "interrupted") => global_types::SessionStatus::Stopped,
        _ => global_types::SessionStatus::Failed,
    }
}

async fn append_item_started(stream_path: &Path, params: &Value) -> Result<()> {
    let Some(item_type) = params.pointer("/item/type").and_then(Value::as_str) else {
        return Ok(());
    };
    if item_type == "commandExecution" || item_type == "fileChange" {
        let name = params
            .pointer("/item/command")
            .or_else(|| params.pointer("/item/path"))
            .and_then(Value::as_str)
            .unwrap_or(item_type);
        append_jsonl(
            stream_path,
            CodexStreamLine(json!({"type": "assistant", "message": {"content": [{"type": "tool_use", "name": name}]}})),
        )
        .await?;
    }
    Ok(())
}

async fn append_item_completed(
    stream_path: &Path,
    params: &Value,
    state: &CodexStreamState,
) -> Result<()> {
    if params.pointer("/item/type").and_then(Value::as_str) != Some("agentMessage") {
        return Ok(());
    }
    if state.agent_message_delta_items.contains(&item_key(params)) {
        return Ok(());
    }
    if let Some(text) = params.pointer("/item/text").and_then(Value::as_str) {
        append_assistant_text(stream_path, text).await?;
    }
    Ok(())
}

async fn append_assistant_text(stream_path: &Path, text: &str) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    append_jsonl(
        stream_path,
        CodexStreamLine(
            json!({"type": "assistant", "message": {"content": [{"type": "text", "text": text}]}}),
        ),
    )
    .await
}

async fn append_tool_result(stream_path: &Path, text: &str) -> Result<()> {
    append_jsonl(
        stream_path,
        CodexStreamLine(json!({"type": "tool_result", "content": text})),
    )
    .await
}

fn error_text(value: &Value) -> String {
    value
        .pointer("/error/message")
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

fn item_key(params: &Value) -> String {
    params
        .pointer("/item/id")
        .or_else(|| params.get("itemId"))
        .and_then(Value::as_str)
        .unwrap_or("__unknown_agent_message__")
        .to_string()
}

#[tracing::instrument(skip(stream_path, state, result_text, structured_output))]
pub(super) async fn append_result(
    stream_path: &Path,
    status: global_types::SessionStatus,
    state: &CodexStreamState,
    total_cost_usd: Option<f64>,
    result_text: Option<&str>,
    structured_output: Option<CodexStructuredOutput>,
    structured_output_error: Option<&str>,
) -> Result<()> {
    let is_error = status == global_types::SessionStatus::Failed;
    append_jsonl(
        stream_path,
        CodexStreamLine(json!({
            "type": "result",
            "subtype": if is_error { "error" } else { "success" },
            "is_error": is_error,
            "duration_ms": state.duration_ms,
            "num_turns": 1,
            "total_cost_usd": total_cost_usd,
            "usage": normalize_usage(state.last_usage.as_ref()),
            "result": result_text.unwrap_or(""),
            "structured_output": structured_output.map(|output| output.0),
            "structured_output_error": structured_output_error,
        })),
    )
    .await
}

fn normalize_usage(usage: Option<&Value>) -> Option<Value> {
    let usage = usage?;
    let total = usage.get("total").unwrap_or(usage);
    let input_tokens = token_count(total, &["inputTokens", "input_tokens"]);
    let output_tokens = token_count(total, &["outputTokens", "output_tokens"]);
    let cache_read_tokens = token_count(total, &["cachedInputTokens", "cache_read_input_tokens"]);
    let cache_creation_tokens = token_count(
        total,
        &["cacheCreationInputTokens", "cache_creation_input_tokens"],
    );

    Some(json!({
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "cache_read_input_tokens": cache_read_tokens,
        "cache_creation_input_tokens": cache_creation_tokens,
    }))
}

fn token_count(value: &Value, names: &[&str]) -> u64 {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(Value::as_u64))
        .unwrap_or(0)
}

#[tracing::instrument(skip(path, line))]
pub(super) async fn append_jsonl(path: &Path, line: CodexStreamLine) -> Result<()> {
    let stream_lock = stream_lock(path);
    let _guard = stream_lock.lock().await;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("create stream directory {}", parent.display()))?;
    }
    let line = serde_json::to_string(&line.0).context("serialize Codex stream line")?;
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .with_context(|| format!("open stream {}", path.display()))?;
    file.write_all(format!("{line}\n").as_bytes())
        .await
        .with_context(|| format!("append stream {}", path.display()))?;
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
            tracing::warn!(module = "codex_app_server", path = %path.display(), "stream lock map poisoned; using one-off lock");
            Arc::new(AsyncMutex::new(()))
        }
    }
}

fn log_append_error(result: Result<()>) {
    if let Err(e) = result {
        tracing::warn!(module = "codex_app_server", error = %e, "failed to append Codex stream line");
    }
}

pub(super) fn collect_agent_text(
    notification: CodexNotification<'_>,
    agent_text: &mut String,
    final_agent_text: &mut Option<String>,
) {
    let value = notification.0;
    let method = value.get("method").and_then(Value::as_str).unwrap_or("");
    let params = value.get("params").unwrap_or(&Value::Null);
    match method {
        "item/agentMessage/delta" => {
            if let Some(delta) = params.get("delta").and_then(Value::as_str) {
                agent_text.push_str(delta);
            }
        }
        "item/completed" => {
            if params.pointer("/item/type").and_then(Value::as_str) == Some("agentMessage") {
                if let Some(text) = params.pointer("/item/text").and_then(Value::as_str) {
                    *final_agent_text = Some(text.to_string());
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn output_delta_notifications_append_tool_result() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("codex.jsonl");
        let mut state = CodexStreamState::default();
        let notification = json!({
            "method": "item/commandExecution/outputDelta",
            "params": {"delta": "hello from command"}
        });

        assert!(!handle_notification(&path, CodexNotification(&notification), &mut state).await);

        let text = read_stream_until(&path, |text| text.contains("hello from command")).await;
        assert!(text.contains("hello from command"));
    }

    #[tokio::test]
    async fn completed_agent_message_is_not_duplicated_after_delta() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("codex.jsonl");
        let mut state = CodexStreamState::default();
        let delta = json!({
            "method": "item/agentMessage/delta",
            "params": {"itemId": "msg-1", "delta": "hello"}
        });
        let completed = json!({
            "method": "item/completed",
            "params": {"item": {"id": "msg-1", "type": "agentMessage", "text": "hello"}}
        });

        assert!(!handle_notification(&path, CodexNotification(&delta), &mut state).await);
        assert!(!handle_notification(&path, CodexNotification(&completed), &mut state).await);

        let text = read_stream_until(&path, |text| text.matches("hello").count() == 1).await;
        assert_eq!(text.matches("hello").count(), 1);
    }

    #[test]
    fn normalize_usage_preserves_cache_creation_tokens() {
        let usage = json!({
            "total": {
                "inputTokens": 10,
                "outputTokens": 20,
                "cachedInputTokens": 3,
                "cacheCreationInputTokens": 4,
            }
        });

        let normalized = normalize_usage(Some(&usage)).unwrap();

        assert_eq!(normalized["input_tokens"], 10);
        assert_eq!(normalized["output_tokens"], 20);
        assert_eq!(normalized["cache_read_input_tokens"], 3);
        assert_eq!(normalized["cache_creation_input_tokens"], 4);
    }

    #[test]
    fn normalize_usage_preserves_snake_case_cache_creation_tokens() {
        let usage = json!({
            "input_tokens": 10,
            "output_tokens": 20,
            "cache_read_input_tokens": 3,
            "cache_creation_input_tokens": 4,
        });

        let normalized = normalize_usage(Some(&usage)).unwrap();

        assert_eq!(normalized["cache_creation_input_tokens"], 4);
    }

    async fn read_stream_until(path: &std::path::Path, predicate: impl Fn(&str) -> bool) -> String {
        for _ in 0..20 {
            let text = tokio::fs::read_to_string(path).await.unwrap_or_default();
            if predicate(&text) {
                return text;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        tokio::fs::read_to_string(path).await.unwrap_or_default()
    }
}
