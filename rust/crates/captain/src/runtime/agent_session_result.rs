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
