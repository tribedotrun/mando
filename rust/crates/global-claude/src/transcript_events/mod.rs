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
    ClaudeProgressKind, EventIndex, EventMeta, HookPhase, McpServerStatus, SystemApiRetryEvent,
    SystemClaudeProgressEvent, SystemCompactBoundaryEvent, SystemHookEvent, SystemInitEvent,
    SystemLocalCommandOutputEvent, SystemRateLimitEvent, SystemStatusEvent,
    SystemThinkingTokensEvent, ToolProgressEvent, TranscriptEvent, UnknownEvent, UserEvent,
};

use crate::transcript_events::helpers::{parse_permission_mode, string_array};

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
            // CC emits `system`/`thinking_tokens` events as progress signals
            // for extended-thinking token estimates. They carry no thinking
            // text — only counters + ids — so they're suppressed in the viewer
            // (see SystemMessage). Parsing them as a named variant keeps the
            // event off the `Unknown` escape hatch and the wire closed-set.
            Some("thinking_tokens") => {
                TranscriptEvent::SystemThinkingTokens(SystemThinkingTokensEvent { meta })
            }
            Some("task_started") => claude_progress(meta, ClaudeProgressKind::TaskStarted),
            Some("task_notification") => {
                claude_progress(meta, ClaudeProgressKind::TaskNotification)
            }
            Some("background_tasks_changed") => {
                claude_progress(meta, ClaudeProgressKind::BackgroundTasksChanged)
            }
            Some("task_updated") => claude_progress(meta, ClaudeProgressKind::TaskUpdated),
            Some("code_change_published") => {
                claude_progress(meta, ClaudeProgressKind::CodeChangePublished)
            }
            Some("vcs_state_changed") => claude_progress(meta, ClaudeProgressKind::VcsStateChanged),
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

fn claude_progress(meta: EventMeta, progress_kind: ClaudeProgressKind) -> TranscriptEvent {
    TranscriptEvent::SystemClaudeProgress(SystemClaudeProgressEvent {
        meta,
        progress_kind,
    })
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
