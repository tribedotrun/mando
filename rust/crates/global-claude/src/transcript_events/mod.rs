//! Typed-event projection of CC session JSONL.
//!
//! Reads the raw JSONL CC produces and emits
//! `api_types::TranscriptEvent` values so callers can render without
//! re-parsing strings. Unlike [`crate::transcript::parse_messages`], this
//! includes every event in the file (not just the last-init-onward slice) so
//! the viewer can surface session resumes as boundary markers instead of
//! truncating history.

mod blocks;
mod helpers;
mod result;
mod tool_inputs;

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use api_types::{
    EventIndex, EventMeta, HookPhase, McpServerStatus, SystemApiRetryEvent,
    SystemCompactBoundaryEvent, SystemHookEvent, SystemInitEvent, SystemLocalCommandOutputEvent,
    SystemRateLimitEvent, SystemStatusEvent, ToolProgressEvent, TranscriptEvent, UnknownEvent,
    UserEvent,
};

use crate::transcript_events::helpers::{parse_permission_mode, string_array};

/// Parse every line in a JSONL stream file into typed transcript events.
///
/// Returns an empty vector if the file is missing or unreadable. Individual
/// malformed lines become `TranscriptEvent::Unknown` variants so no history is
/// silently dropped.
pub fn parse_events(stream_path: &Path) -> Vec<TranscriptEvent> {
    parse_events_with_size(stream_path).0
}

/// Parse the stream file once and return events alongside the byte length
/// and the total number of input lines (including empty lines that the
/// parser skipped). Callers tailing live sessions feed the byte length back
/// into `parse_events_from_offset` so lines appended between
/// `parse_events(...)` and a separate `stream_file_size(...)` call are never
/// silently skipped (the two-read race flagged on PR #975); the line count
/// keeps `EventIndex.line_number` metadata aligned with the source file
/// even when empty lines are present (the undercount flagged on PR #975).
pub fn parse_events_with_size(stream_path: &Path) -> (Vec<TranscriptEvent>, u64, u32) {
    let content = match std::fs::read_to_string(stream_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!(
                path = %stream_path.display(),
                error = %e,
                "cannot read stream file for event parse",
            );
            return (Vec::new(), 0, 0);
        }
    };
    let size = content.len() as u64;
    let line_count = content.lines().count().try_into().unwrap_or(u32::MAX);
    let events = parse_events_from_str(&content, 1);
    (events, size, line_count)
}

/// Parse events starting from a byte offset, returning new events plus the
/// byte offset reached.
///
/// The caller uses the returned offset to resume tailing without re-parsing
/// previously-emitted events. If the offset lands mid-line, the partial line
/// is dropped (the next call will re-see it once the line completes).
pub fn parse_events_from_offset(
    stream_path: &Path,
    byte_offset: u64,
    starting_line_number: u32,
) -> (Vec<TranscriptEvent>, u64) {
    let mut file = match std::fs::File::open(stream_path) {
        Ok(f) => f,
        Err(e) => {
            tracing::debug!(
                path = %stream_path.display(),
                error = %e,
                "cannot open stream file for tail parse",
            );
            return (Vec::new(), byte_offset);
        }
    };
    let total_len = match file.metadata() {
        Ok(m) => m.len(),
        Err(e) => {
            tracing::debug!(
                path = %stream_path.display(),
                error = %e,
                "cannot stat stream file for tail parse",
            );
            return (Vec::new(), byte_offset);
        }
    };
    if total_len <= byte_offset {
        return (Vec::new(), byte_offset);
    }
    if let Err(e) = file.seek(SeekFrom::Start(byte_offset)) {
        tracing::debug!(
            path = %stream_path.display(),
            error = %e,
            "cannot seek stream file for tail parse",
        );
        return (Vec::new(), byte_offset);
    }
    let mut buf = Vec::with_capacity((total_len - byte_offset) as usize);
    if let Err(e) = file.read_to_end(&mut buf) {
        tracing::debug!(
            path = %stream_path.display(),
            error = %e,
            "cannot read stream tail for tail parse",
        );
        return (Vec::new(), byte_offset);
    }
    let content = String::from_utf8_lossy(&buf);
    let complete_len = match content.rfind('\n') {
        Some(idx) => idx + 1,
        None => return (Vec::new(), byte_offset),
    };
    let complete_slice = &content[..complete_len];
    let events = parse_events_from_str(complete_slice, starting_line_number);
    (events, byte_offset + complete_len as u64)
}

/// Size of a file in bytes, or `0` when the file is missing/unreadable.
pub fn stream_file_size(stream_path: &Path) -> u64 {
    std::fs::metadata(stream_path).map(|m| m.len()).unwrap_or(0)
}

fn parse_events_from_str(content: &str, starting_line: u32) -> Vec<TranscriptEvent> {
    let mut events = Vec::new();
    for (offset, line) in content.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let line_number = starting_line.saturating_add(offset as u32);
        let event = match serde_json::from_str::<serde_json::Value>(line) {
            Ok(val) => parse_event_value(&val, line, line_number),
            Err(e) => {
                tracing::debug!(
                    line_number,
                    error = %e,
                    "skipping malformed JSONL line in event parse",
                );
                TranscriptEvent::Unknown(UnknownEvent {
                    meta: EventMeta {
                        index: EventIndex { line_number },
                        uuid: None,
                        parent_uuid: None,
                        session_id: None,
                        timestamp: None,
                        is_sidechain: None,
                    },
                    raw_type: None,
                    raw_subtype: None,
                    raw: line.to_string(),
                })
            }
        };
        events.push(event);
    }
    events
}

fn parse_event_value(val: &serde_json::Value, raw_line: &str, line_number: u32) -> TranscriptEvent {
    let meta = build_meta(val, line_number);
    let raw_type = val.get("type").and_then(|v| v.as_str()).map(String::from);
    let raw_subtype = val
        .get("subtype")
        .and_then(|v| v.as_str())
        .map(String::from);

    match raw_type.as_deref() {
        Some("system") => match raw_subtype.as_deref() {
            Some("init") => TranscriptEvent::SystemInit(parse_system_init(val, meta)),
            Some("compact_boundary") => {
                TranscriptEvent::SystemCompactBoundary(SystemCompactBoundaryEvent {
                    meta,
                    reason: val.get("reason").and_then(|v| v.as_str()).map(String::from),
                })
            }
            Some("status") => TranscriptEvent::SystemStatus(SystemStatusEvent {
                meta,
                status: val.get("status").and_then(|v| v.as_str()).map(String::from),
                message: val
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(String::from),
            }),
            Some("api_retry") => TranscriptEvent::SystemApiRetry(SystemApiRetryEvent {
                meta,
                message: val
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                retry_in_ms: val.get("retry_in_ms").and_then(|v| v.as_u64()),
                attempt: val
                    .get("attempt")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32),
            }),
            Some("local_command_output") => {
                TranscriptEvent::SystemLocalCommandOutput(SystemLocalCommandOutputEvent {
                    meta,
                    command: val
                        .get("command")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    output: val
                        .get("output")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                })
            }
            Some("hook_started") => {
                TranscriptEvent::SystemHook(parse_hook(val, meta, HookPhase::Started))
            }
            Some("hook_response") => {
                TranscriptEvent::SystemHook(parse_hook(val, meta, HookPhase::Response))
            }
            _ => unknown(meta, raw_type, raw_subtype, raw_line),
        },
        Some("rate_limit_event") => TranscriptEvent::SystemRateLimit(SystemRateLimitEvent {
            meta,
            info: val
                .get("rate_limit_info")
                .map(|v| v.to_string())
                .unwrap_or_default(),
        }),
        Some("user") => TranscriptEvent::User(UserEvent {
            meta,
            blocks: blocks::parse_user_blocks(val),
        }),
        Some("assistant") => TranscriptEvent::Assistant(blocks::parse_assistant(val, meta)),
        Some("tool_progress") => TranscriptEvent::ToolProgress(ToolProgressEvent {
            meta,
            tool_use_id: val
                .get("tool_use_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            tool_name: tool_inputs::parse_tool_name(
                val.get("tool_name").and_then(|v| v.as_str()).unwrap_or(""),
            ),
            elapsed_seconds: val.get("elapsed_time_seconds").and_then(|v| v.as_f64()),
        }),
        Some("result") => {
            TranscriptEvent::Result(result::parse_result(val, meta, raw_subtype.as_deref()))
        }
        _ => unknown(meta, raw_type, raw_subtype, raw_line),
    }
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

fn build_meta(val: &serde_json::Value, line_number: u32) -> EventMeta {
    EventMeta {
        index: EventIndex { line_number },
        uuid: val.get("uuid").and_then(|v| v.as_str()).map(String::from),
        parent_uuid: val
            .get("parentUuid")
            .and_then(|v| v.as_str())
            .map(String::from),
        session_id: val
            .get("session_id")
            .and_then(|v| v.as_str())
            .or_else(|| val.get("sessionId").and_then(|v| v.as_str()))
            .map(String::from),
        timestamp: val
            .get("timestamp")
            .and_then(|v| v.as_str())
            .map(String::from),
        is_sidechain: val.get("isSidechain").and_then(|v| v.as_bool()),
    }
}

fn parse_hook(val: &serde_json::Value, meta: EventMeta, phase: HookPhase) -> SystemHookEvent {
    SystemHookEvent {
        meta,
        phase,
        hook_id: val
            .get("hook_id")
            .and_then(|v| v.as_str())
            .map(String::from),
        hook_name: val
            .get("hook_name")
            .and_then(|v| v.as_str())
            .map(String::from),
        hook_event: val
            .get("hook_event")
            .and_then(|v| v.as_str())
            .map(String::from),
        output: val.get("output").and_then(|v| v.as_str()).map(String::from),
        stdout: val.get("stdout").and_then(|v| v.as_str()).map(String::from),
        stderr: val.get("stderr").and_then(|v| v.as_str()).map(String::from),
    }
}

fn parse_system_init(val: &serde_json::Value, meta: EventMeta) -> SystemInitEvent {
    SystemInitEvent {
        meta,
        cwd: val.get("cwd").and_then(|v| v.as_str()).map(String::from),
        model: val.get("model").and_then(|v| v.as_str()).map(String::from),
        permission_mode: val
            .get("permissionMode")
            .and_then(|v| v.as_str())
            .and_then(parse_permission_mode),
        tools: string_array(val.get("tools")),
        slash_commands: string_array(val.get("slash_commands")),
        mcp_servers: val
            .get("mcp_servers")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|entry| {
                        let name = entry.get("name").and_then(|v| v.as_str())?.to_string();
                        let status = entry
                            .get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        Some(McpServerStatus { name, status })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        output_style: val
            .get("output_style")
            .and_then(|v| v.as_str())
            .map(String::from),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_types::{AssistantContentBlock, ResultOutcome, ToolInput, ToolName, UserContentBlock};

    fn temp_file(content: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mando-events-{}", std::process::id()));
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join(format!(
            "test-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn parse_init_event_emits_typed_variant() {
        let line = r#"{"type":"system","subtype":"init","session_id":"s1","uuid":"u0","cwd":"/tmp","model":"claude-haiku-4-5","permissionMode":"acceptEdits","tools":["Read","Bash"],"slash_commands":["/help"],"mcp_servers":[{"name":"tribe","status":"connected"}]}"#;
        let path = temp_file(line);
        let events = parse_events(&path);
        assert_eq!(events.len(), 1);
        match &events[0] {
            TranscriptEvent::SystemInit(init) => {
                assert_eq!(init.cwd.as_deref(), Some("/tmp"));
                assert_eq!(
                    init.permission_mode,
                    Some(api_types::CcPermissionMode::AcceptEdits)
                );
                assert_eq!(init.tools, vec!["Read".to_string(), "Bash".to_string()]);
                assert_eq!(init.mcp_servers.len(), 1);
                assert_eq!(init.mcp_servers[0].name, "tribe");
            }
            other => panic!("expected SystemInit, got {other:?}"),
        }
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn parse_assistant_with_tool_use_maps_named_tool() {
        let line = r#"{"type":"assistant","uuid":"a1","message":{"model":"opus","content":[{"type":"text","text":"hi"},{"type":"tool_use","id":"tu1","name":"Bash","input":{"command":"ls -la","description":"list"}}]}}"#;
        let path = temp_file(line);
        let events = parse_events(&path);
        assert_eq!(events.len(), 1);
        let TranscriptEvent::Assistant(evt) = &events[0] else {
            panic!("expected Assistant");
        };
        assert_eq!(evt.model.as_deref(), Some("opus"));
        assert_eq!(evt.blocks.len(), 2);
        let AssistantContentBlock::ToolUse(tool) = &evt.blocks[1] else {
            panic!("expected tool_use block");
        };
        assert!(matches!(tool.name, ToolName::Bash));
        match &tool.input {
            ToolInput::Bash(b) => {
                assert_eq!(b.command, "ls -la");
                assert_eq!(b.description.as_deref(), Some("list"));
            }
            other => panic!("expected Bash input, got {other:?}"),
        }
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn parse_mcp_tool_splits_server_and_tool() {
        let line = r#"{"type":"assistant","uuid":"a1","message":{"content":[{"type":"tool_use","id":"tu","name":"mcp__tribe__ask","input":{"question":"hi"}}]}}"#;
        let path = temp_file(line);
        let events = parse_events(&path);
        let TranscriptEvent::Assistant(evt) = &events[0] else {
            panic!("expected Assistant");
        };
        let AssistantContentBlock::ToolUse(tool) = &evt.blocks[0] else {
            panic!("expected tool_use block");
        };
        match &tool.name {
            ToolName::Mcp(m) => {
                assert_eq!(m.server, "tribe");
                assert_eq!(m.tool, "ask");
            }
            other => panic!("expected Mcp, got {other:?}"),
        }
        match &tool.input {
            ToolInput::Opaque(o) => assert!(o.raw.contains("\"question\"")),
            other => panic!("expected Opaque, got {other:?}"),
        }
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn parse_user_tool_result_blocks() {
        let line = r#"{"type":"user","uuid":"u1","message":{"content":[{"type":"tool_result","tool_use_id":"tu1","content":"ok","is_error":false}]}}"#;
        let path = temp_file(line);
        let events = parse_events(&path);
        let TranscriptEvent::User(evt) = &events[0] else {
            panic!("expected User");
        };
        let UserContentBlock::ToolResult(tr) = &evt.blocks[0] else {
            panic!("expected tool_result block");
        };
        assert_eq!(tr.tool_use_id, "tu1");
        assert_eq!(tr.is_error, Some(false));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn parse_result_event_maps_outcome() {
        let line = r#"{"type":"result","subtype":"success","uuid":"r","duration_ms":100,"num_turns":3,"total_cost_usd":0.01,"usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}"#;
        let path = temp_file(line);
        let events = parse_events(&path);
        let TranscriptEvent::Result(evt) = &events[0] else {
            panic!("expected Result");
        };
        assert!(matches!(evt.outcome, ResultOutcome::Success));
        assert_eq!(evt.summary.num_turns, Some(3));
        assert_eq!(evt.summary.total_cost_usd, Some(0.01));
        assert!(!evt.summary.is_error);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn parse_malformed_line_becomes_unknown_event() {
        let line = "not-json\n";
        let path = temp_file(line);
        let events = parse_events(&path);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], TranscriptEvent::Unknown(_)));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn parse_events_from_offset_returns_new_lines_only() {
        let lines1 = r#"{"type":"system","subtype":"init","uuid":"u1"}"#;
        let path = temp_file(&format!("{lines1}\n"));

        let (events1, offset1) = parse_events_from_offset(&path, 0, 1);
        assert_eq!(events1.len(), 1);

        // Append another line.
        let line2 = r#"{"type":"user","uuid":"u2","message":{"content":"hi"}}"#;
        {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            writeln!(f, "{line2}").unwrap();
        }

        let (events2, _offset2) = parse_events_from_offset(&path, offset1, 2);
        assert_eq!(events2.len(), 1);
        assert!(matches!(events2[0], TranscriptEvent::User(_)));

        std::fs::remove_file(&path).ok();
    }
}
