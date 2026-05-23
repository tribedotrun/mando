//! Provider-neutral session result polling.
//!
//! Runtime adapters write provider-specific output into Mando's stream files;
//! Captain lifecycle code consumes this neutral projection instead of parsing
//! Claude/Codex protocol details directly.

use serde_json::Value;

#[derive(Debug, Clone)]
pub(crate) enum AgentSessionPoll {
    Pending,
    Failed(String),
    UnusableOutput(String),
    Completed(AgentSessionOutput),
}

#[derive(Debug, Clone)]
pub(crate) enum AgentSessionOutput {
    Structured {
        value: Value,
        fallback_text: Option<String>,
    },
    Text(String),
}

/// Poll a session whose result is expected to drive a typed captain decision.
///
/// Codex text-mode sessions intentionally do not use this helper: a Codex
/// result without `structured_output` is treated as unusable here so
/// clarifier/review/merge lanes fail closed instead of silently consuming
/// free-form text.
pub(crate) fn poll_structured_session_output(
    provider: global_types::TaskProvider,
    session_id: &str,
) -> AgentSessionPoll {
    let stream_path = stream_path(provider, session_id);
    if let Some(result) = global_claude::get_stream_result(&stream_path) {
        if result.get("is_error").and_then(|v| v.as_bool()) == Some(true) {
            return AgentSessionPoll::Failed(
                result
                    .get("error")
                    .and_then(|v| v.as_str())
                    .or_else(|| result.get("result").and_then(|v| v.as_str()))
                    .unwrap_or("agent process failed")
                    .to_string(),
            );
        }

        if let Some(structured) = result.get("structured_output").filter(|v| !v.is_null()) {
            let fallback_text = match provider {
                global_types::TaskProvider::Claude => fallback_text(&result, &stream_path),
                global_types::TaskProvider::Codex => None,
            };
            return AgentSessionPoll::Completed(AgentSessionOutput::Structured {
                value: structured.clone(),
                fallback_text,
            });
        }

        if matches!(provider, global_types::TaskProvider::Codex) {
            let reason = result
                .get("structured_output_error")
                .and_then(|v| v.as_str())
                .unwrap_or("agent session completed without required structured_output");
            return AgentSessionPoll::UnusableOutput(reason.to_string());
        }

        if let Some(text) = fallback_text(&result, &stream_path).filter(|s| !s.is_empty()) {
            return AgentSessionPoll::Completed(AgentSessionOutput::Text(text));
        }

        return AgentSessionPoll::UnusableOutput(
            "agent session completed but produced no extractable output".to_string(),
        );
    }

    if is_session_finished(provider, session_id) {
        if matches!(provider, global_types::TaskProvider::Codex) {
            return AgentSessionPoll::UnusableOutput(
                "structured-output agent session finished without result".to_string(),
            );
        }
        if let Some(text) = global_claude::get_last_assistant_text(&stream_path) {
            return AgentSessionPoll::Completed(AgentSessionOutput::Text(text));
        }
        return AgentSessionPoll::UnusableOutput(
            "agent session finished without result or assistant text".to_string(),
        );
    }

    AgentSessionPoll::Pending
}

pub(crate) fn session_output_text(output: AgentSessionOutput) -> String {
    match output {
        AgentSessionOutput::Structured { value, .. } => value.to_string(),
        AgentSessionOutput::Text(text) => text,
    }
}

pub(crate) fn stream_path(
    provider: global_types::TaskProvider,
    session_id: &str,
) -> std::path::PathBuf {
    match provider {
        global_types::TaskProvider::Claude => {
            global_infra::paths::stream_path_for_session(session_id)
        }
        global_types::TaskProvider::Codex => {
            global_infra::paths::codex_derived_stream_path_for_session(session_id)
        }
    }
}

pub(crate) fn stream_meta_path(
    provider: global_types::TaskProvider,
    session_id: &str,
) -> std::path::PathBuf {
    match provider {
        global_types::TaskProvider::Claude => {
            global_infra::paths::stream_meta_path_for_session(session_id)
        }
        global_types::TaskProvider::Codex => {
            global_infra::paths::codex_derived_stream_meta_path_for_session(session_id)
        }
    }
}

pub(crate) fn is_session_finished(provider: global_types::TaskProvider, session_id: &str) -> bool {
    match provider {
        global_types::TaskProvider::Claude => global_claude::is_session_finished(session_id),
        global_types::TaskProvider::Codex => {
            global_claude::is_stream_meta_finished_at(&stream_meta_path(provider, session_id))
        }
    }
}

pub(crate) fn stream_file_size(provider: global_types::TaskProvider, session_id: &str) -> u64 {
    global_claude::get_stream_file_size(&stream_path(provider, session_id))
}

fn fallback_text(result: &Value, stream_path: &std::path::Path) -> Option<String> {
    result
        .get("result")
        .and_then(|v| v.as_str())
        .map(String::from)
        .filter(|s| !s.is_empty())
        .or_else(|| global_claude::get_last_assistant_text(stream_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn isolated_stream(session_id: &str, result: serde_json::Value) {
        let stream_path = global_infra::paths::codex_derived_stream_path_for_session(session_id);
        if let Some(parent) = stream_path.parent() {
            tokio::fs::create_dir_all(parent).await.unwrap();
        }
        let init = serde_json::json!({"type":"system","subtype":"init"});
        let body = format!("{init}\n{result}\n");
        tokio::fs::write(stream_path, body).await.unwrap();
    }

    #[tokio::test]
    async fn structured_poller_accepts_codex_structured_result() {
        let _lock = global_infra::PROCESS_ENV_LOCK.lock().await;
        let temp = tempfile::tempdir().unwrap();
        let _guard = global_infra::EnvVarGuard::set("MANDO_DATA_DIR", temp.path());
        let session_id = "codex-structured-result";
        isolated_stream(
            session_id,
            serde_json::json!({
                "type": "result",
                "subtype": "success",
                "is_error": false,
                "result": "",
                "structured_output": {"action": "ship", "feedback": "ok"}
            }),
        )
        .await;

        match poll_structured_session_output(global_types::TaskProvider::Codex, session_id) {
            AgentSessionPoll::Completed(AgentSessionOutput::Structured {
                value,
                fallback_text,
            }) => {
                assert_eq!(value["action"], "ship");
                assert!(fallback_text.is_none());
            }
            other => panic!("expected structured Codex result, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn structured_poller_rejects_codex_text_only_result() {
        let _lock = global_infra::PROCESS_ENV_LOCK.lock().await;
        let temp = tempfile::tempdir().unwrap();
        let _guard = global_infra::EnvVarGuard::set("MANDO_DATA_DIR", temp.path());
        let session_id = "codex-text-only-result";
        isolated_stream(
            session_id,
            serde_json::json!({
                "type": "result",
                "subtype": "success",
                "is_error": false,
                "result": "plain worker transcript",
                "structured_output": null
            }),
        )
        .await;

        match poll_structured_session_output(global_types::TaskProvider::Codex, session_id) {
            AgentSessionPoll::UnusableOutput(reason) => {
                assert!(reason.contains("structured_output"));
            }
            other => panic!("expected Codex text-only result to be rejected, got {other:?}"),
        }
    }
}
