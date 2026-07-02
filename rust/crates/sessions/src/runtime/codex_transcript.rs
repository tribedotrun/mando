//! Typed-event projection of Mando-owned Codex app-server JSONL.
//!
//! Raw stdout JSONL is captured under `state/session-jsonl/codex/`; unknown
//! lines become `Unknown` events so the renderer never silently drops data.

use std::io::Read;
use std::path::Path;

use api_types::{
    AssistantContentBlock, AssistantEvent, AssistantTextBlock, AssistantThinkingBlock, EventIndex,
    EventMeta, ModelUsageBreakdown, ResultEvent, ResultOutcome, ResultSummary,
    SystemLocalCommandOutputEvent, SystemStatusEvent, TranscriptEvent, UnknownEvent,
};
use serde_json::Value;

use super::codex_item_events::{
    item_notification_events, turn_item_events, usage_info, CodexParseState, CodexValue,
};

pub fn parse_events_with_size(stream_path: &Path) -> (Vec<TranscriptEvent>, u64, u32) {
    let content = match std::fs::read_to_string(stream_path) {
        Ok(content) => content,
        Err(e) => {
            tracing::debug!(
                path = %stream_path.display(),
                error = %e,
                "cannot read Codex app-server JSONL for event parse",
            );
            return (Vec::new(), 0, 0);
        }
    };
    let size = content.len() as u64;
    let line_count = content.lines().count().try_into().unwrap_or(u32::MAX);
    let events = parse_events_from_str(&content, 1);
    (events, size, line_count)
}

pub fn parse_events_from_offset(
    stream_path: &Path,
    byte_offset: u64,
    starting_line_number: u32,
) -> (Vec<TranscriptEvent>, u64) {
    let mut file = match std::fs::File::open(stream_path) {
        Ok(file) => file,
        Err(e) => {
            tracing::debug!(
                path = %stream_path.display(),
                error = %e,
                "cannot open Codex app-server JSONL for tail parse",
            );
            return (Vec::new(), byte_offset);
        }
    };
    let mut buf = Vec::new();
    if let Err(e) = file.read_to_end(&mut buf) {
        tracing::debug!(
            path = %stream_path.display(),
            error = %e,
            "cannot read Codex app-server JSONL tail",
        );
        return (Vec::new(), byte_offset);
    }
    let start = match usize::try_from(byte_offset) {
        Ok(start) if start < buf.len() => start,
        _ => return (Vec::new(), byte_offset),
    };
    let tail = String::from_utf8_lossy(&buf[start..]);
    let complete_tail_len = match tail.rfind('\n') {
        Some(index) => index + 1,
        None => return (Vec::new(), byte_offset),
    };
    let complete_len = start.saturating_add(complete_tail_len);
    let content = String::from_utf8_lossy(&buf[..complete_len]);
    let events = parse_events_from_str_min_line(&content, 1, starting_line_number);
    (events, complete_len as u64)
}

fn parse_events_from_str(content: &str, starting_line: u32) -> Vec<TranscriptEvent> {
    parse_events_from_str_min_line(content, starting_line, starting_line)
}

fn parse_events_from_str_min_line(
    content: &str,
    starting_line: u32,
    min_emit_line: u32,
) -> Vec<TranscriptEvent> {
    let mut events = Vec::new();
    let mut state = CodexParseState::default();
    for (offset, line) in content.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let line_number = starting_line.saturating_add(offset as u32);
        let line_events = match serde_json::from_str::<Value>(line) {
            Ok(value) => parse_line_events(&value, line, line_number, &mut state),
            Err(e) => {
                tracing::debug!(
                    line_number,
                    error = %e,
                    "malformed Codex app-server JSONL line",
                );
                vec![unknown(empty_meta(line_number), None, None, line)]
            }
        };
        events.extend(
            line_events
                .into_iter()
                .filter(|event| event_line_number(event) >= min_emit_line),
        );
    }
    events
}

fn parse_line_events(
    value: &Value,
    raw_line: &str,
    line_number: u32,
    state: &mut CodexParseState,
) -> Vec<TranscriptEvent> {
    let meta = build_meta(value, line_number);
    state.record_model_from_value(CodexValue(value));
    state.record_usage_from_value(CodexValue(value));
    if value.get("id").and_then(Value::as_u64).is_some() {
        let mut events = turn_item_events(CodexValue(value), &meta, state);
        events.push(unknown(
            meta,
            Some("response".into()),
            response_status(value),
            raw_line,
        ));
        return events;
    }

    let method = value.get("method").and_then(Value::as_str);
    match method {
        Some("item/agentMessage/delta") => vec![assistant_text(value, meta, raw_line, state)],
        Some("item/reasoning/summaryTextDelta" | "item/reasoning/textDelta") => {
            vec![assistant_thinking(value, meta, raw_line)]
        }
        Some(
            "item/commandExecution/outputDelta"
            | "item/fileChange/outputDelta"
            | "item/commandExecution/output/delta"
            | "item/fileChange/output/delta",
        ) => vec![local_output(value, meta, raw_line)],
        Some("item/started" | "item/completed") => {
            let mut events = item_notification_events(CodexValue(value), &meta, state);
            if events.is_empty() {
                events.push(unknown(
                    meta,
                    method.map(str::to_string),
                    raw_subtype(value),
                    raw_line,
                ));
            }
            events
        }
        Some("turn/completed") => {
            let mut events = turn_item_events(CodexValue(value), &meta, state);
            events.push(result_event(value, meta, state));
            events
        }
        Some("error") => vec![error_status(value, meta)],
        Some(
            "turn/started" | "thread/started" | "thread/status/changed" | "thread/settings/updated",
        ) => {
            let mut events = turn_item_events(CodexValue(value), &meta, state);
            events.push(status_event(value, meta));
            events
        }
        Some("thread/tokenUsage/updated") => vec![unknown(
            meta,
            method.map(str::to_string),
            raw_subtype(value),
            raw_line,
        )],
        _ => vec![unknown(
            meta,
            method.map(str::to_string),
            raw_subtype(value),
            raw_line,
        )],
    }
}

fn event_line_number(event: &TranscriptEvent) -> u32 {
    event.line_number()
}

fn assistant_text(
    value: &Value,
    meta: EventMeta,
    raw_line: &str,
    state: &mut CodexParseState,
) -> TranscriptEvent {
    let Some(text) = string_at(value, &["/params/delta"]) else {
        tracing::warn!(
            module = "sessions",
            line_number = meta.index.line_number,
            "Codex app-server agent message delta missing params.delta",
        );
        return unknown(
            meta,
            Some("item/agentMessage/delta".into()),
            raw_subtype(value),
            raw_line,
        );
    };
    state.record_agent_delta(string_at(value, &["/params/itemId", "/params/item/id"]).as_deref());
    TranscriptEvent::Assistant(AssistantEvent {
        model: string_at(value, &["/params/model", "/params/item/model"]).or_else(|| state.model()),
        blocks: vec![AssistantContentBlock::Text(AssistantTextBlock { text })],
        usage: None,
        stop_reason: None,
        meta,
    })
}

fn assistant_thinking(value: &Value, meta: EventMeta, raw_line: &str) -> TranscriptEvent {
    let Some(text) = string_at(value, &["/params/delta"]) else {
        tracing::warn!(
            module = "sessions",
            line_number = meta.index.line_number,
            "Codex app-server reasoning delta missing params.delta",
        );
        return unknown(
            meta,
            value
                .get("method")
                .and_then(Value::as_str)
                .map(str::to_string),
            raw_subtype(value),
            raw_line,
        );
    };
    TranscriptEvent::Assistant(AssistantEvent {
        model: string_at(value, &["/params/model", "/params/item/model"]),
        blocks: vec![AssistantContentBlock::Thinking(AssistantThinkingBlock {
            text,
        })],
        usage: None,
        stop_reason: None,
        meta,
    })
}

fn local_output(value: &Value, meta: EventMeta, raw_line: &str) -> TranscriptEvent {
    let Some(output) = string_at(value, &["/params/delta"]) else {
        tracing::warn!(
            module = "sessions",
            line_number = meta.index.line_number,
            "Codex app-server output delta missing params.delta",
        );
        return unknown(
            meta,
            value
                .get("method")
                .and_then(Value::as_str)
                .map(str::to_string),
            raw_subtype(value),
            raw_line,
        );
    };
    TranscriptEvent::SystemLocalCommandOutput(SystemLocalCommandOutputEvent {
        command: string_at(value, &["/params/item/command", "/params/item/path"]),
        output,
        meta,
    })
}

fn status_event(value: &Value, meta: EventMeta) -> TranscriptEvent {
    let status = string_at(
        value,
        &[
            "/params/status",
            "/params/thread/status",
            "/params/turn/status",
        ],
    )
    .or_else(|| {
        value
            .get("method")
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    let message = string_at(
        value,
        &[
            "/params/message",
            "/params/thread/title",
            "/params/title",
            "/params/thread/name",
        ],
    );
    TranscriptEvent::SystemStatus(SystemStatusEvent {
        meta,
        status,
        message,
    })
}

fn error_status(value: &Value, meta: EventMeta) -> TranscriptEvent {
    TranscriptEvent::SystemStatus(SystemStatusEvent {
        meta,
        status: Some("error".into()),
        message: error_text(value.pointer("/params").unwrap_or(value))
            .filter(|text| !text.is_empty()),
    })
}

fn result_event(value: &Value, meta: EventMeta, state: &CodexParseState) -> TranscriptEvent {
    let params = value.get("params").unwrap_or(value);
    let turn = params.get("turn").unwrap_or(params);
    let status = turn.get("status").and_then(Value::as_str);
    let error = turn.get("error").filter(|value| !value.is_null());
    let is_error = error.is_some() || !matches!(status, Some("completed" | "interrupted"));
    let usage = usage_info(
        params
            .get("tokenUsage")
            .or_else(|| turn.get("tokenUsage"))
            .or_else(|| params.get("usage"))
            .map(CodexValue),
    )
    .or_else(|| state.last_usage());
    let model = string_at(value, &["/params/model", "/params/turn/model"])
        .or_else(|| state.model())
        .unwrap_or_else(|| "codex".into());
    let cost_usd = usage
        .as_ref()
        .and_then(|usage| estimated_cost_usd(&model, usage));
    let model_usage = usage
        .clone()
        .map(|usage| ModelUsageBreakdown {
            model,
            usage,
            cost_usd,
            context_window: state.context_window(),
        })
        .into_iter()
        .collect();
    TranscriptEvent::Result(ResultEvent {
        meta,
        outcome: if is_error {
            ResultOutcome::ErrorDuringExecution
        } else {
            ResultOutcome::Success
        },
        summary: ResultSummary {
            duration_ms: u64_at(turn, &["/durationMs", "/duration_ms"]),
            duration_api_ms: None,
            num_turns: Some(1),
            total_cost_usd: cost_usd,
            stop_reason: status.map(str::to_string),
            permission_denials: Vec::new(),
            errors: error
                .and_then(error_text)
                .filter(|text| !text.is_empty())
                .into_iter()
                .collect(),
            usage,
            model_usage,
            is_error,
        },
    })
}

fn estimated_cost_usd(model: &str, usage: &api_types::TranscriptUsageInfo) -> Option<f64> {
    let cost = global_claude::rate_for_model(model).cost_for(
        usage.input_tokens,
        usage.output_tokens,
        usage.cache_creation_tokens,
        usage.cache_read_tokens,
    );
    (cost > 0.0).then_some(cost)
}

fn build_meta(value: &Value, line_number: u32) -> EventMeta {
    EventMeta {
        index: EventIndex { line_number },
        uuid: string_at(
            value,
            &[
                "/params/item/id",
                "/params/itemId",
                "/params/turn/id",
                "/result/turn/id",
                "/result/thread/id",
                "/id",
            ],
        )
        .map(|id| {
            if value.get("id").and_then(Value::as_u64).is_some() {
                format!("response-{id}")
            } else {
                id
            }
        }),
        parent_uuid: None,
        session_id: string_at(
            value,
            &[
                "/params/threadId",
                "/params/turn/threadId",
                "/result/thread/id",
                "/result/turn/threadId",
                "/result/threadId",
            ],
        ),
        timestamp: string_at(value, &["/params/timestamp", "/timestamp"]),
        is_sidechain: None,
    }
}

fn empty_meta(line_number: u32) -> EventMeta {
    EventMeta {
        index: EventIndex { line_number },
        uuid: None,
        parent_uuid: None,
        session_id: None,
        timestamp: None,
        is_sidechain: None,
    }
}

fn raw_subtype(value: &Value) -> Option<String> {
    string_at(
        value,
        &[
            "/params/item/type",
            "/params/turn/status",
            "/params/thread/status",
            "/params/status",
        ],
    )
}

fn response_status(value: &Value) -> Option<String> {
    if value.get("error").is_some() {
        Some("error".into())
    } else {
        string_at(value, &["/result/thread/status", "/result/turn/status"])
            .or_else(|| Some("ok".into()))
    }
}

fn string_at(value: &Value, pointers: &[&str]) -> Option<String> {
    pointers
        .iter()
        .find_map(|pointer| value.pointer(pointer).and_then(Value::as_str))
        .map(str::to_string)
}

fn u64_at(value: &Value, pointers: &[&str]) -> Option<u64> {
    pointers.iter().find_map(|pointer| {
        let value = value.pointer(pointer)?;
        value.as_u64().or_else(|| {
            value
                .as_i64()
                .and_then(|num| (num >= 0).then_some(num as u64))
        })
    })
}

fn error_text(value: &Value) -> Option<String> {
    value
        .pointer("/error/message")
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| Some(value.to_string()))
}

fn unknown(
    meta: EventMeta,
    raw_type: Option<String>,
    raw_subtype: Option<String>,
    raw_line: &str,
) -> TranscriptEvent {
    TranscriptEvent::Unknown(UnknownEvent {
        meta,
        raw_type,
        raw_subtype,
        raw: raw_line.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(content: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "mando-codex-events-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn json_rpc_response_becomes_unknown_raw_event() {
        let path =
            temp_file(r#"{"id":101,"result":{"thread":{"id":"thread-1","status":"ready"}}}"#);

        let events = parse_events_with_size(&path).0;

        let TranscriptEvent::Unknown(event) = &events[0] else {
            panic!("expected unknown response event");
        };
        assert_eq!(event.raw_type.as_deref(), Some("response"));
        assert_eq!(event.meta.session_id.as_deref(), Some("thread-1"));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn agent_message_delta_becomes_assistant_text() {
        let path = temp_file(
            r#"{"method":"item/agentMessage/delta","params":{"threadId":"thread-1","itemId":"msg-1","delta":"hello"}}"#,
        );

        let events = parse_events_with_size(&path).0;

        let TranscriptEvent::Assistant(event) = &events[0] else {
            panic!("expected assistant event");
        };
        let AssistantContentBlock::Text(text) = &event.blocks[0] else {
            panic!("expected text block");
        };
        assert_eq!(event.meta.uuid.as_deref(), Some("msg-1"));
        assert_eq!(text.text, "hello");
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn turn_completed_uses_prior_token_usage() {
        let path = temp_file(
            r#"{"id":101,"result":{"model":"gpt-5.4","thread":{"id":"thread-1","status":"ready"}}}
{"method":"thread/tokenUsage/updated","params":{"threadId":"thread-1","turnId":"turn-1","tokenUsage":{"total":{"inputTokens":7,"outputTokens":3,"cachedInputTokens":2,"cacheCreationInputTokens":1},"modelContextWindow":1000}}}
{"method":"turn/completed","params":{"threadId":"thread-1","turn":{"id":"turn-1","status":"completed","durationMs":42}}}"#,
        );

        let events = parse_events_with_size(&path).0;
        let result = events
            .iter()
            .find_map(|event| match event {
                TranscriptEvent::Result(event) => Some(event),
                _ => None,
            })
            .expect("result event");

        assert_eq!(result.summary.duration_ms, Some(42));
        assert_eq!(
            result
                .summary
                .usage
                .as_ref()
                .map(|usage| usage.input_tokens),
            Some(7)
        );
        assert!(result.summary.total_cost_usd.is_some());
        assert_eq!(result.summary.model_usage[0].context_window, Some(1000));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn missing_delta_becomes_unknown_event() {
        let path = temp_file(
            r#"{"method":"item/agentMessage/delta","params":{"threadId":"thread-1","itemId":"msg-1"}}"#,
        );

        let events = parse_events_with_size(&path).0;

        let TranscriptEvent::Unknown(event) = &events[0] else {
            panic!("expected unknown event");
        };
        assert_eq!(event.raw_type.as_deref(), Some("item/agentMessage/delta"));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn turn_items_become_user_and_tool_events() {
        let path = temp_file(
            r#"{"id":102,"result":{"turn":{"id":"turn-1","threadId":"thread-1","status":"completed","items":[{"id":"u1","type":"userMessage","content":[{"type":"text","text":"run tests"}]},{"id":"cmd1","type":"commandExecution","command":"cargo test","aggregatedOutput":"ok","status":"completed","exitCode":0}]}}}"#,
        );

        let events = parse_events_with_size(&path).0;

        assert!(events.iter().any(|event| matches!(event, TranscriptEvent::User(user) if matches!(user.blocks.first(), Some(api_types::UserContentBlock::Text(_))))));
        assert!(events.iter().any(|event| matches!(event, TranscriptEvent::Assistant(assistant) if matches!(assistant.blocks.first(), Some(AssistantContentBlock::ToolUse(_))))));
        assert!(events.iter().any(|event| matches!(event, TranscriptEvent::User(user) if matches!(user.blocks.first(), Some(api_types::UserContentBlock::ToolResult(_))))));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn stateful_offset_parse_preserves_prior_usage_for_late_completion() {
        let content = r#"{"method":"thread/tokenUsage/updated","params":{"threadId":"thread-1","turnId":"turn-1","tokenUsage":{"total":{"inputTokens":7,"outputTokens":3}}}}
{"method":"turn/completed","params":{"threadId":"thread-1","turn":{"id":"turn-1","status":"completed"}}}
"#;
        let path = temp_file(content);
        let first_newline = content.find('\n').unwrap() + 1;

        let (events, _) = parse_events_from_offset(&path, first_newline as u64, 2);

        let TranscriptEvent::Result(event) = &events[0] else {
            panic!("expected result event");
        };
        assert_eq!(
            event.summary.usage.as_ref().map(|usage| usage.input_tokens),
            Some(7)
        );
        std::fs::remove_file(path).ok();
    }
}
