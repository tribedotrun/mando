use api_types::{
    AssistantContentBlock, AssistantEvent, AssistantToolUseBlock, BashInput, EventMeta,
    FileChangeEntry, FileChangeInput, FileChangeKind, ImageViewInput, McpToolName, OpaqueInput,
    OtherToolName, TaskInput, ToolInput, ToolName, ToolResultContent, ToolResultText,
    TranscriptEvent, UserContentBlock, UserEvent, UserToolResultBlock, WebSearchInput,
};
use serde_json::Value;

use super::{item_id, string_at, CodexParseState, CodexValue, ItemSource};

pub(super) fn command_events(
    item: CodexValue<'_>,
    meta: EventMeta,
    state: &mut CodexParseState,
    source: ItemSource,
) -> Vec<TranscriptEvent> {
    let item = item.0;
    let input = ToolInput::Bash(BashInput {
        command: string_at(item, &["/commandActions/0/command", "/command"]).unwrap_or_default(),
        description: None,
        timeout: None,
        run_in_background: None,
    });
    tool_events(
        item,
        meta,
        state,
        source,
        ToolName::Bash,
        input,
        command_result_text,
    )
}

pub(super) fn file_change_events(
    item: CodexValue<'_>,
    meta: EventMeta,
    state: &mut CodexParseState,
    source: ItemSource,
) -> Vec<TranscriptEvent> {
    let item = item.0;
    let changes = item
        .get("changes")
        .and_then(Value::as_array)
        .map(|changes| {
            changes
                .iter()
                .filter_map(|change| {
                    let path = string_at(change, &["/path"])?;
                    Some(FileChangeEntry {
                        path: state.relative_path(path),
                        kind: file_change_kind(change),
                        move_path: string_at(change, &["/kind/move_path"])
                            .map(|path| state.relative_path(path)),
                        diff: string_at(change, &["/diff"]).unwrap_or_default(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if changes.is_empty() {
        return Vec::new();
    }
    tool_events(
        item,
        meta,
        state,
        source,
        ToolName::FileChange,
        ToolInput::FileChange(FileChangeInput { changes }),
        no_result_text,
    )
}

pub(super) fn image_view_events(
    item: CodexValue<'_>,
    meta: EventMeta,
    state: &mut CodexParseState,
    source: ItemSource,
) -> Vec<TranscriptEvent> {
    let item = item.0;
    let Some(path) = string_at(item, &["/path"]) else {
        return Vec::new();
    };
    tool_events(
        item,
        meta,
        state,
        source,
        ToolName::ImageView,
        ToolInput::ImageView(ImageViewInput {
            path: state.relative_path(path),
        }),
        no_result_text,
    )
}

pub(super) fn subagent_events(
    item: CodexValue<'_>,
    meta: EventMeta,
    state: &mut CodexParseState,
    source: ItemSource,
) -> Vec<TranscriptEvent> {
    let item = item.0;
    let Some(agent_thread_id) = string_at(item, &["/agentThreadId"]) else {
        return Vec::new();
    };
    let agent_path = string_at(item, &["/agentPath"]).unwrap_or_else(|| "subagent".into());
    let agent_name = agent_path
        .rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or("subagent")
        .to_string();
    emit_tool_events(
        item,
        meta,
        state,
        source,
        ToolEventSpec {
            id: agent_thread_id,
            name: ToolName::Task,
            input: ToolInput::Task(TaskInput {
                description: agent_name.clone(),
                prompt: String::new(),
                subagent_type: Some(agent_name),
            }),
            result_text: subagent_result_text,
        },
    )
}

pub(super) fn mcp_tool_events(
    item: CodexValue<'_>,
    meta: EventMeta,
    state: &mut CodexParseState,
    source: ItemSource,
) -> Vec<TranscriptEvent> {
    let item = item.0;
    let server = string_at(item, &["/server"]).unwrap_or_else(|| "mcp".into());
    let tool = string_at(item, &["/tool"]).unwrap_or_else(|| "tool".into());
    let name = ToolName::Mcp(McpToolName { server, tool });
    let input = ToolInput::Opaque(OpaqueInput {
        raw: item
            .get("arguments")
            .and_then(|value| serde_json::to_string(value).ok())
            .unwrap_or_else(|| "{}".into()),
    });
    tool_events(item, meta, state, source, name, input, generic_result_text)
}

pub(super) fn web_search_events(
    item: CodexValue<'_>,
    meta: EventMeta,
    state: &mut CodexParseState,
    source: ItemSource,
) -> Vec<TranscriptEvent> {
    let item = item.0;
    let input = ToolInput::WebSearch(WebSearchInput {
        query: string_at(item, &["/query"]).unwrap_or_default(),
        allowed_domains: None,
        blocked_domains: None,
    });
    tool_events(
        item,
        meta,
        state,
        source,
        ToolName::WebSearch,
        input,
        generic_result_text,
    )
}

pub(super) fn opaque_tool_events(
    item: CodexValue<'_>,
    meta: EventMeta,
    state: &mut CodexParseState,
    source: ItemSource,
    fallback_name: &str,
) -> Vec<TranscriptEvent> {
    let item = item.0;
    let name = ToolName::Other(OtherToolName {
        name: string_at(item, &["/tool"]).unwrap_or_else(|| fallback_name.into()),
    });
    let input = ToolInput::Opaque(OpaqueInput {
        raw: serde_json::to_string(item).unwrap_or_else(|_| "{}".into()),
    });
    tool_events(item, meta, state, source, name, input, generic_result_text)
}

fn file_change_kind(change: &Value) -> FileChangeKind {
    match change.pointer("/kind/type").and_then(Value::as_str) {
        Some("add" | "create") => FileChangeKind::Add,
        Some("update") => FileChangeKind::Update,
        Some("delete") => FileChangeKind::Delete,
        Some("move" | "rename") => FileChangeKind::Move,
        _ => FileChangeKind::Other,
    }
}

fn tool_events(
    item: &Value,
    meta: EventMeta,
    state: &mut CodexParseState,
    source: ItemSource,
    name: ToolName,
    input: ToolInput,
    result_text: fn(&Value) -> Option<String>,
) -> Vec<TranscriptEvent> {
    let Some(id) = item_id(item) else {
        return Vec::new();
    };
    emit_tool_events(
        item,
        meta,
        state,
        source,
        ToolEventSpec {
            id,
            name,
            input,
            result_text,
        },
    )
}

struct ToolEventSpec {
    id: String,
    name: ToolName,
    input: ToolInput,
    result_text: fn(&Value) -> Option<String>,
}

fn emit_tool_events(
    item: &Value,
    meta: EventMeta,
    state: &mut CodexParseState,
    source: ItemSource,
    spec: ToolEventSpec,
) -> Vec<TranscriptEvent> {
    let ToolEventSpec {
        id,
        name,
        input,
        result_text,
    } = spec;
    let mut events = Vec::new();
    if !state.tool_started_items.contains(&id) {
        state.tool_started_items.insert(id.clone());
        events.push(TranscriptEvent::Assistant(AssistantEvent {
            meta: meta.clone(),
            model: state.model(),
            blocks: vec![AssistantContentBlock::ToolUse(AssistantToolUseBlock {
                id: id.clone(),
                name,
                input,
            })],
            usage: None,
            stop_reason: None,
        }));
    }
    if source != ItemSource::Started {
        if let Some(text) = result_text(item) {
            events.push(TranscriptEvent::User(UserEvent {
                meta,
                blocks: vec![UserContentBlock::ToolResult(UserToolResultBlock {
                    tool_use_id: id,
                    content: ToolResultContent::Text(ToolResultText { text }),
                    is_error: Some(item_failed(item)),
                })],
            }));
        }
    }
    events
}

fn no_result_text(_: &Value) -> Option<String> {
    None
}

fn subagent_result_text(item: &Value) -> Option<String> {
    matches!(item.get("kind").and_then(Value::as_str), Some("completed"))
        .then(|| "completed".to_string())
}

fn command_result_text(item: &Value) -> Option<String> {
    string_at(item, &["/aggregatedOutput"]).or_else(|| {
        terminal_status(item).map(|status| {
            let command = string_at(item, &["/command"]).unwrap_or_default();
            if command.is_empty() {
                status
            } else {
                format!("{command}\n{status}")
            }
        })
    })
}

fn generic_result_text(item: &Value) -> Option<String> {
    item.get("result")
        .or_else(|| item.get("error"))
        .or_else(|| item.get("changes"))
        .or_else(|| item.get("contentItems"))
        .and_then(|value| serde_json::to_string(value).ok())
        .or_else(|| terminal_status(item))
}

fn item_failed(item: &Value) -> bool {
    matches!(
        item.get("status").and_then(Value::as_str),
        Some("failed" | "error" | "errored")
    ) || item.get("error").is_some()
        || item
            .get("exitCode")
            .and_then(Value::as_i64)
            .is_some_and(|code| code != 0)
}

fn terminal_status(item: &Value) -> Option<String> {
    let status = item.get("status").and_then(Value::as_str)?;
    (status != "inProgress" && status != "in_progress").then(|| format!("status: {status}"))
}
