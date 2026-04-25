//! User / assistant content-block parsers.

use api_types::{
    AssistantContentBlock, AssistantEvent, AssistantTextBlock, AssistantThinkingBlock,
    AssistantToolUseBlock, EventMeta, ToolResultBlocks, ToolResultChildBlock, ToolResultContent,
    ToolResultText, ToolResultUnknownBlock, UserContentBlock, UserImageBlock, UserTextBlock,
    UserToolResultBlock,
};

use crate::transcript_events::helpers::parse_usage;
use crate::transcript_events::tool_inputs::{parse_tool_input, parse_tool_name};

pub(super) fn parse_user_blocks(val: &serde_json::Value) -> Vec<UserContentBlock> {
    let content = match val.pointer("/message/content") {
        Some(c) => c,
        None => return Vec::new(),
    };
    match content {
        serde_json::Value::String(text) => {
            vec![UserContentBlock::Text(UserTextBlock { text: text.clone() })]
        }
        serde_json::Value::Array(blocks) => blocks
            .iter()
            .filter_map(parse_user_block)
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    }
}

fn parse_user_block(block: &serde_json::Value) -> Option<UserContentBlock> {
    let kind = block.get("type").and_then(|v| v.as_str())?;
    match kind {
        "text" => Some(UserContentBlock::Text(UserTextBlock {
            text: block
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
        })),
        "image" => Some(UserContentBlock::Image(parse_image_block(block))),
        "tool_result" => Some(UserContentBlock::ToolResult(UserToolResultBlock {
            tool_use_id: block
                .get("tool_use_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            content: parse_tool_result_content(block.get("content")),
            is_error: block.get("is_error").and_then(|v| v.as_bool()),
        })),
        _ => None,
    }
}

fn parse_image_block(block: &serde_json::Value) -> UserImageBlock {
    let media_type = block
        .pointer("/source/media_type")
        .and_then(|v| v.as_str())
        .map(String::from);
    let data_len = block
        .pointer("/source/data")
        .and_then(|v| v.as_str())
        .map(|s| s.len() as u64);
    UserImageBlock {
        media_type,
        data_len,
    }
}

fn parse_tool_result_content(content: Option<&serde_json::Value>) -> ToolResultContent {
    match content {
        Some(serde_json::Value::String(text)) => {
            ToolResultContent::Text(ToolResultText { text: text.clone() })
        }
        Some(serde_json::Value::Array(arr)) => {
            let blocks = arr.iter().map(parse_tool_result_child).collect();
            ToolResultContent::Blocks(ToolResultBlocks { blocks })
        }
        _ => ToolResultContent::Text(ToolResultText {
            text: String::new(),
        }),
    }
}

fn parse_tool_result_child(block: &serde_json::Value) -> ToolResultChildBlock {
    let kind = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match kind {
        "text" => ToolResultChildBlock::Text(ToolResultText {
            text: block
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
        }),
        "image" => ToolResultChildBlock::Image(parse_image_block(block)),
        _ => ToolResultChildBlock::Unknown(ToolResultUnknownBlock {
            raw: serde_json::to_string(block).unwrap_or_default(),
        }),
    }
}

pub(super) fn parse_assistant(val: &serde_json::Value, meta: EventMeta) -> AssistantEvent {
    let blocks = match val.pointer("/message/content") {
        Some(serde_json::Value::Array(arr)) => {
            arr.iter().filter_map(parse_assistant_block).collect()
        }
        Some(serde_json::Value::String(text)) => {
            vec![AssistantContentBlock::Text(AssistantTextBlock {
                text: text.clone(),
            })]
        }
        _ => Vec::new(),
    };
    let model = val
        .pointer("/message/model")
        .and_then(|v| v.as_str())
        .map(String::from);
    let usage = val.pointer("/message/usage").map(parse_usage);
    let stop_reason = val
        .pointer("/message/stop_reason")
        .and_then(|v| v.as_str())
        .map(String::from);
    AssistantEvent {
        meta,
        model,
        blocks,
        usage,
        stop_reason,
    }
}

fn parse_assistant_block(block: &serde_json::Value) -> Option<AssistantContentBlock> {
    let kind = block.get("type").and_then(|v| v.as_str())?;
    match kind {
        "text" => Some(AssistantContentBlock::Text(AssistantTextBlock {
            text: block
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
        })),
        "thinking" => Some(AssistantContentBlock::Thinking(AssistantThinkingBlock {
            text: block
                .get("thinking")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
        })),
        "tool_use" => Some(AssistantContentBlock::ToolUse(parse_tool_use(block))),
        _ => None,
    }
}

fn parse_tool_use(block: &serde_json::Value) -> AssistantToolUseBlock {
    let raw_name = block.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let input_val = block.get("input").unwrap_or(&serde_json::Value::Null);
    let name = parse_tool_name(raw_name);
    let input = parse_tool_input(&name, input_val);
    AssistantToolUseBlock {
        id: block
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        name,
        input,
    }
}
