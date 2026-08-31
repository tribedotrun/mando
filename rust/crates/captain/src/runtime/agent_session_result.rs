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
    super::agent_runtime::Adapter::new(provider).poll(session_id)
}

pub(super) fn poll_for_adapter(
    adapter: super::agent_runtime::Adapter,
    session_id: &str,
) -> AgentSessionPoll {
    let stream_path = adapter.stream_path(session_id);
    if let Some(result) = adapter.result(&stream_path) {
        let result = result.0;
        let outcome = agent_runtime_core::result_outcome(&result);
        if outcome == api_types::ResultOutcome::Interrupted {
            return AgentSessionPoll::UnusableOutput(
                "agent session was interrupted before completion".to_string(),
            );
        }
        if outcome.is_error() {
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
            let fallback_text = adapter
                .is_claude()
                .then(|| fallback_text(&result, &stream_path))
                .flatten();
            return AgentSessionPoll::Completed(AgentSessionOutput::Structured {
                value: structured.clone(),
                fallback_text,
            });
        }

        if adapter.requires_structured_output() {
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

    if adapter.is_finished(session_id) {
        if adapter.requires_structured_output() {
            return AgentSessionPoll::UnusableOutput(
                "structured-output agent session finished without result".to_string(),
            );
        }
        if let Some(text) = agent_runtime_core::get_last_assistant_text(&stream_path) {
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
    super::agent_runtime::Adapter::new(provider).stream_path(session_id)
}

pub(crate) fn stream_meta_path(
    provider: global_types::TaskProvider,
    session_id: &str,
) -> std::path::PathBuf {
    super::agent_runtime::Adapter::new(provider).stream_meta_path(session_id)
}

pub(crate) fn stream_file_size(provider: global_types::TaskProvider, session_id: &str) -> u64 {
    agent_runtime_core::get_stream_file_size(&stream_path(provider, session_id))
}

pub(crate) fn record_interrupted_result(
    provider: global_types::TaskProvider,
    stream_path: &std::path::Path,
) {
    super::agent_runtime::Adapter::new(provider).record_interrupted_result(stream_path);
}

pub(crate) fn should_record_interrupted_result(
    provider: global_types::TaskProvider,
    stream_path: &std::path::Path,
) -> bool {
    super::agent_runtime::Adapter::new(provider).should_record_interrupted_result(stream_path)
}

fn fallback_text(result: &Value, stream_path: &std::path::Path) -> Option<String> {
    result
        .get("result")
        .and_then(|v| v.as_str())
        .map(String::from)
        .filter(|s| !s.is_empty())
        .or_else(|| agent_runtime_core::get_last_assistant_text(stream_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_stop_overrides_a_provider_error_with_interrupted() {
        let temp = tempfile::tempdir().unwrap();
        let stream_path = temp.path().join("claude-stream.jsonl");
        std::fs::write(
            &stream_path,
            concat!(
                "{\"type\":\"system\",\"subtype\":\"init\"}\n",
                "{\"type\":\"result\",\"subtype\":\"error_during_execution\",\"is_error\":true}\n"
            ),
        )
        .unwrap();

        record_interrupted_result(global_types::TaskProvider::Claude, &stream_path);

        let result = agent_runtime_core::get_stream_result(&stream_path).unwrap();
        assert_eq!(
            agent_runtime_core::result_outcome(&result),
            api_types::ResultOutcome::Interrupted
        );
    }

    #[test]
    fn completed_claude_session_is_not_rewritten_as_interrupted() {
        let temp = tempfile::tempdir().unwrap();
        let stream_path = temp.path().join("claude-stream.jsonl");
        std::fs::write(
            &stream_path,
            concat!(
                "{\"type\":\"system\",\"subtype\":\"init\"}\n",
                "{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false}\n"
            ),
        )
        .unwrap();

        assert!(!should_record_interrupted_result(
            global_types::TaskProvider::Claude,
            &stream_path
        ));
    }

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

    #[tokio::test]
    async fn structured_poller_does_not_complete_interrupted_result() {
        let _lock = global_infra::PROCESS_ENV_LOCK.lock().await;
        let temp = tempfile::tempdir().unwrap();
        let _guard = global_infra::EnvVarGuard::set("MANDO_DATA_DIR", temp.path());
        let session_id = "codex-interrupted-result";
        isolated_stream(
            session_id,
            serde_json::json!({
                "type": "result",
                "subtype": "interrupted",
                "is_error": false,
                "structured_output": {"action": "ship"}
            }),
        )
        .await;

        match poll_structured_session_output(global_types::TaskProvider::Codex, session_id) {
            AgentSessionPoll::UnusableOutput(reason) => {
                assert!(reason.contains("interrupted"));
            }
            other => panic!("expected interrupted result to be unusable, got {other:?}"),
        }
    }
}
