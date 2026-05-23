use std::collections::HashSet;

use api_types::{
    AssistantContentBlock, AssistantEvent, AssistantTextBlock, AssistantThinkingBlock,
    AssistantToolUseBlock, BashInput, EventMeta, McpToolName, OpaqueInput, OtherToolName,
    SystemStatusEvent, ToolInput, ToolName, ToolResultContent, ToolResultText, TranscriptEvent,
    TranscriptUsageInfo, UserContentBlock, UserEvent, UserImageBlock, UserTextBlock,
    UserToolResultBlock, WebSearchInput,
};
use serde_json::Value;

pub(super) struct CodexValue<'a>(pub(super) &'a Value);

#[derive(Default)]
pub(super) struct CodexParseState {
    agent_message_delta_items: HashSet<String>,
    tool_started_items: HashSet<String>,
    last_usage: Option<TranscriptUsageInfo>,
    model: Option<String>,
    context_window: Option<u64>,
}

impl CodexParseState {
    pub(super) fn record_agent_delta(&mut self, item_id: Option<&str>) {
        if let Some(item_id) = item_id.filter(|item_id| !item_id.is_empty()) {
            self.agent_message_delta_items.insert(item_id.to_string());
        }
    }

    pub(super) fn record_model_from_value(&mut self, value: CodexValue<'_>) {
        let value = value.0;
        if self.model.is_some() {
            return;
        }
        self.model = string_at(
            value,
            &[
                "/result/model",
                "/result/thread/model",
                "/params/model",
                "/params/thread/model",
                "/params/turn/model",
            ],
        );
    }

    pub(super) fn record_usage_from_value(&mut self, value: CodexValue<'_>) {
        let value = value.0;
        let usage = value
            .pointer("/params/tokenUsage")
            .or_else(|| value.pointer("/params/turn/tokenUsage"))
            .or_else(|| value.pointer("/params/usage"))
            .or_else(|| value.pointer("/result/turn/tokenUsage"))
            .or_else(|| value.pointer("/result/tokenUsage"));
        if let Some(usage) = usage {
            self.last_usage = usage_info(Some(CodexValue(usage)));
            self.context_window = u64_at(
                usage,
                &[
                    "/modelContextWindow",
                    "/model_context_window",
                    "/total/modelContextWindow",
                ],
            );
        }
    }

    pub(super) fn last_usage(&self) -> Option<TranscriptUsageInfo> {
        self.last_usage.clone()
    }

    pub(super) fn model(&self) -> Option<String> {
        self.model.clone()
    }

    pub(super) fn context_window(&self) -> Option<u64> {
        self.context_window
    }
}

pub(super) fn usage_info(value: Option<CodexValue<'_>>) -> Option<TranscriptUsageInfo> {
    let usage = value?.0;
    let total = usage.get("total").unwrap_or(usage);
    Some(TranscriptUsageInfo {
        input_tokens: token_count(total, &["inputTokens", "input_tokens"]),
        output_tokens: token_count(total, &["outputTokens", "output_tokens"]),
        cache_read_tokens: token_count(total, &["cachedInputTokens", "cache_read_input_tokens"]),
        cache_creation_tokens: token_count(
            total,
            &["cacheCreationInputTokens", "cache_creation_input_tokens"],
        ),
    })
}

pub(super) fn turn_item_events(
    value: CodexValue<'_>,
    base_meta: &EventMeta,
    state: &mut CodexParseState,
) -> Vec<TranscriptEvent> {
    value
        .0
        .pointer("/result/turn/items")
        .or_else(|| value.0.pointer("/params/turn/items"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .flat_map(|item| item_events(item, base_meta, state, ItemSource::Snapshot))
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn item_notification_events(
    value: CodexValue<'_>,
    base_meta: &EventMeta,
    state: &mut CodexParseState,
) -> Vec<TranscriptEvent> {
    let value = value.0;
    let Some(item) = value.pointer("/params/item") else {
        return Vec::new();
    };
    let source = match value.get("method").and_then(Value::as_str) {
        Some("item/started") => ItemSource::Started,
        Some("item/completed") => ItemSource::Completed,
        _ => ItemSource::Snapshot,
    };
    item_events(item, base_meta, state, source)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ItemSource {
    Started,
    Completed,
    Snapshot,
}

fn item_events(
    item: &Value,
    base_meta: &EventMeta,
    state: &mut CodexParseState,
    source: ItemSource,
) -> Vec<TranscriptEvent> {
    let meta = item_meta(base_meta, item);
    match item.get("type").and_then(Value::as_str) {
        Some("userMessage") => user_message_event(item, meta).into_iter().collect(),
        Some("agentMessage") => agent_message_event(item, meta, state).into_iter().collect(),
        Some("plan") | Some("reasoning") => thinking_event(item, meta).into_iter().collect(),
        Some("commandExecution") => command_events(item, meta, state, source),
        Some("fileChange") => opaque_tool_events(item, meta, state, source, "fileChange"),
        Some("mcpToolCall") => mcp_tool_events(item, meta, state, source),
        Some("dynamicToolCall") => opaque_tool_events(item, meta, state, source, "dynamicToolCall"),
        Some("collabAgentToolCall") => {
            opaque_tool_events(item, meta, state, source, "collabAgentToolCall")
        }
        Some("webSearch") => web_search_events(item, meta, state, source),
        Some("imageGeneration") => opaque_tool_events(item, meta, state, source, "imageGeneration"),
        Some("error") => error_item_event(item, meta).into_iter().collect(),
        _ => Vec::new(),
    }
}

fn user_message_event(item: &Value, meta: EventMeta) -> Option<TranscriptEvent> {
    let blocks = user_blocks(item);
    (!blocks.is_empty()).then_some(TranscriptEvent::User(UserEvent { meta, blocks }))
}

fn user_blocks(item: &Value) -> Vec<UserContentBlock> {
    match item.get("content") {
        Some(Value::Array(items)) => items.iter().filter_map(user_block).collect(),
        Some(Value::String(text)) => vec![UserContentBlock::Text(UserTextBlock {
            text: text.to_string(),
        })],
        _ => Vec::new(),
    }
}

fn user_block(value: &Value) -> Option<UserContentBlock> {
    match value.get("type").and_then(Value::as_str) {
        Some("text") => string_at(value, &["/text"]).map(|text| {
            UserContentBlock::Text(UserTextBlock {
                text: text.to_string(),
            })
        }),
        Some("image" | "localImage") => Some(UserContentBlock::Image(UserImageBlock {
            media_type: None,
            data_len: None,
        })),
        Some("skill" | "mention") => serde_json::to_string(value)
            .ok()
            .map(|text| UserContentBlock::Text(UserTextBlock { text })),
        _ => None,
    }
}

fn agent_message_event(
    item: &Value,
    meta: EventMeta,
    state: &CodexParseState,
) -> Option<TranscriptEvent> {
    let item_id = item_id(item);
    if item_id
        .as_deref()
        .is_some_and(|id| state.agent_message_delta_items.contains(id))
    {
        return None;
    }
    let text = string_at(item, &["/text"])?;
    (!text.is_empty()).then_some(TranscriptEvent::Assistant(AssistantEvent {
        meta,
        model: state.model(),
        blocks: vec![AssistantContentBlock::Text(AssistantTextBlock { text })],
        usage: None,
        stop_reason: None,
    }))
}

fn thinking_event(item: &Value, meta: EventMeta) -> Option<TranscriptEvent> {
    let text = string_at(item, &["/text"])
        .or_else(|| string_array_text(item.get("summary")))
        .or_else(|| string_array_text(item.get("content")))?;
    (!text.is_empty()).then_some(TranscriptEvent::Assistant(AssistantEvent {
        meta,
        model: None,
        blocks: vec![AssistantContentBlock::Thinking(AssistantThinkingBlock {
            text,
        })],
        usage: None,
        stop_reason: None,
    }))
}

fn command_events(
    item: &Value,
    meta: EventMeta,
    state: &mut CodexParseState,
    source: ItemSource,
) -> Vec<TranscriptEvent> {
    let name = ToolName::Bash;
    let input = ToolInput::Bash(BashInput {
        command: string_at(item, &["/command"]).unwrap_or_default(),
        description: None,
        timeout: None,
        run_in_background: None,
    });
    tool_events(item, meta, state, source, name, input, command_result_text)
}

fn mcp_tool_events(
    item: &Value,
    meta: EventMeta,
    state: &mut CodexParseState,
    source: ItemSource,
) -> Vec<TranscriptEvent> {
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

fn web_search_events(
    item: &Value,
    meta: EventMeta,
    state: &mut CodexParseState,
    source: ItemSource,
) -> Vec<TranscriptEvent> {
    let name = ToolName::WebSearch;
    let input = ToolInput::WebSearch(WebSearchInput {
        query: string_at(item, &["/query"]).unwrap_or_default(),
        allowed_domains: None,
        blocked_domains: None,
    });
    tool_events(item, meta, state, source, name, input, generic_result_text)
}

fn opaque_tool_events(
    item: &Value,
    meta: EventMeta,
    state: &mut CodexParseState,
    source: ItemSource,
    fallback_name: &str,
) -> Vec<TranscriptEvent> {
    let name = ToolName::Other(OtherToolName {
        name: string_at(item, &["/tool"]).unwrap_or_else(|| fallback_name.into()),
    });
    let input = ToolInput::Opaque(OpaqueInput {
        raw: serde_json::to_string(item).unwrap_or_else(|_| "{}".into()),
    });
    tool_events(item, meta, state, source, name, input, generic_result_text)
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

fn error_item_event(item: &Value, meta: EventMeta) -> Option<TranscriptEvent> {
    Some(TranscriptEvent::SystemStatus(SystemStatusEvent {
        meta,
        status: Some("error".into()),
        message: string_at(item, &["/message"]).or_else(|| serde_json::to_string(item).ok()),
    }))
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

fn item_meta(base: &EventMeta, item: &Value) -> EventMeta {
    let mut meta = base.clone();
    meta.uuid = item_id(item).or(meta.uuid);
    meta
}

fn item_id(item: &Value) -> Option<String> {
    string_at(item, &["/id"])
}

fn string_array_text(value: Option<&Value>) -> Option<String> {
    let text = value?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join("\n");
    (!text.is_empty()).then_some(text)
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

fn token_count(value: &Value, names: &[&str]) -> u64 {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(Value::as_u64))
        .unwrap_or(0)
}
