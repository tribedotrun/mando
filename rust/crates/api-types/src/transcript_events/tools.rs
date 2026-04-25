//! Tool name and input discriminated unions.
//!
//! Each variant in `ToolName` has a paired variant in `ToolInput` so the
//! renderer can narrow on the input alone without threading both fields.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Closed-set enum of tools Mando's transcript viewer renders. The `Mcp`
/// variant carries the `server` and `tool` fragments split out from CC's
/// `mcp__<server>__<tool>` naming; `Other` is a catalogued escape for tools
/// the daemon does not recognize so the viewer can still render them.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ToolName {
    Bash,
    Read,
    Edit,
    Write,
    Grep,
    Glob,
    TodoWrite,
    WebFetch,
    WebSearch,
    Task,
    NotebookEdit,
    Skill,
    StructuredOutput,
    Mcp(McpToolName),
    /// Catalogued escape — see `.ai/guardrail-allowlists/internal-value.txt`
    /// under `transcript-events`. Unknown tool names surface here so the
    /// viewer can still render them.
    Other(OtherToolName),
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpToolName {
    pub server: String,
    pub tool: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OtherToolName {
    pub name: String,
}

/// Input payload for a tool_use block. Discriminated by `kind` which always
/// matches the outer `ToolName` variant — duplicated so TypeScript callers
/// can narrow on the input alone without threading both fields.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ToolInput {
    Bash(BashInput),
    Read(ReadInput),
    Edit(EditInput),
    Write(WriteInput),
    Grep(GrepInput),
    Glob(GlobInput),
    TodoWrite(TodoWriteInput),
    WebFetch(WebFetchInput),
    WebSearch(WebSearchInput),
    Task(TaskInput),
    NotebookEdit(NotebookEditInput),
    Skill(SkillInput),
    StructuredOutput(StructuredOutputInput),
    /// Catalogued escape — stringified JSON so the renderer can display the
    /// raw payload. Used for MCP tools (server-specific input shapes) and
    /// unknown tools.
    Opaque(OpaqueInput),
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BashInput {
    pub command: String,
    pub description: Option<String>,
    #[ts(type = "number | null")]
    pub timeout: Option<u64>,
    pub run_in_background: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadInput {
    pub file_path: String,
    #[ts(type = "number | null")]
    pub offset: Option<u64>,
    #[ts(type = "number | null")]
    pub limit: Option<u64>,
    pub pages: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditInput {
    pub file_path: String,
    pub old_string: String,
    pub new_string: String,
    pub replace_all: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteInput {
    pub file_path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrepInput {
    pub pattern: String,
    pub path: Option<String>,
    pub glob: Option<String>,
    pub file_type: Option<String>,
    pub output_mode: Option<GrepOutputMode>,
    #[ts(type = "number | null")]
    pub head_limit: Option<u64>,
    pub case_insensitive: Option<bool>,
    pub multiline: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum GrepOutputMode {
    Content,
    FilesWithMatches,
    Count,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GlobInput {
    pub pattern: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TodoWriteInput {
    pub todos: Vec<CcTodoItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CcTodoItem {
    pub content: String,
    pub active_form: Option<String>,
    pub status: CcTodoItemStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CcTodoItemStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebFetchInput {
    pub url: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebSearchInput {
    pub query: String,
    pub allowed_domains: Option<Vec<String>>,
    pub blocked_domains: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskInput {
    pub description: String,
    pub prompt: String,
    pub subagent_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotebookEditInput {
    pub notebook_path: String,
    pub new_source: String,
    pub cell_id: Option<String>,
    pub cell_type: Option<NotebookCellType>,
    pub edit_mode: Option<NotebookEditMode>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum NotebookCellType {
    Code,
    Markdown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum NotebookEditMode {
    Replace,
    Insert,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillInput {
    pub skill: String,
    pub args: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StructuredOutputInput {
    pub raw: String,
}

/// Stringified JSON, used for tools with server-specific input shapes (MCP)
/// or tool names Mando does not recognize.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpaqueInput {
    pub raw: String,
}
