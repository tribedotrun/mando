//! User and assistant message events plus their content-block unions.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{EventMeta, ToolInput, ToolName};
use crate::TranscriptUsageInfo;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserEvent {
    pub meta: EventMeta,
    pub blocks: Vec<UserContentBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssistantEvent {
    pub meta: EventMeta,
    pub model: Option<String>,
    pub blocks: Vec<AssistantContentBlock>,
    pub usage: Option<TranscriptUsageInfo>,
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum UserContentBlock {
    Text(UserTextBlock),
    Image(UserImageBlock),
    ToolResult(UserToolResultBlock),
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserTextBlock {
    pub text: String,
}

/// Image content-block. The raw base64 never crosses the wire — callers who
/// need pixels fetch the `attachment_id` through a dedicated route.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserImageBlock {
    pub media_type: Option<String>,
    #[ts(type = "number | null")]
    pub data_len: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserToolResultBlock {
    pub tool_use_id: String,
    pub content: ToolResultContent,
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ToolResultContent {
    Text(ToolResultText),
    Blocks(ToolResultBlocks),
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolResultText {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolResultBlocks {
    pub blocks: Vec<ToolResultChildBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ToolResultChildBlock {
    Text(ToolResultText),
    Image(UserImageBlock),
    /// Any shape we do not explicitly model. Carried as stringified JSON so
    /// the renderer can still display it; catalogued escape.
    Unknown(ToolResultUnknownBlock),
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolResultUnknownBlock {
    pub raw: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum AssistantContentBlock {
    Text(AssistantTextBlock),
    Thinking(AssistantThinkingBlock),
    ToolUse(AssistantToolUseBlock),
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssistantTextBlock {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssistantThinkingBlock {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssistantToolUseBlock {
    pub id: String,
    pub name: ToolName,
    pub input: ToolInput,
}
