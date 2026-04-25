//! Tool-name and tool-input parsers.
//!
//! CC emits `{type: "tool_use", name, input: {...}}`; this module maps the
//! raw `name` to a `ToolName` variant and dispatches per-tool input parsing.

use api_types::{
    BashInput, CcTodoItem, CcTodoItemStatus, EditInput, GlobInput, GrepInput, GrepOutputMode,
    McpToolName, NotebookCellType, NotebookEditInput, NotebookEditMode, OpaqueInput, OtherToolName,
    ReadInput, SkillInput, StructuredOutputInput, TaskInput, TodoWriteInput, ToolInput, ToolName,
    WebFetchInput, WebSearchInput, WriteInput,
};

use crate::transcript_events::helpers::{opt_str, opt_string_array, str_field};

pub(super) fn parse_tool_name(raw: &str) -> ToolName {
    if let Some(rest) = raw.strip_prefix("mcp__") {
        let mut parts = rest.splitn(2, "__");
        let server = parts.next().unwrap_or_default().to_string();
        let tool = parts.next().unwrap_or_default().to_string();
        return ToolName::Mcp(McpToolName { server, tool });
    }
    match raw {
        "Bash" | "bash" => ToolName::Bash,
        "Read" | "file_read" => ToolName::Read,
        "Edit" | "file_edit" => ToolName::Edit,
        "Write" | "file_write" => ToolName::Write,
        "Grep" => ToolName::Grep,
        "Glob" => ToolName::Glob,
        "TodoWrite" | "todo_write" => ToolName::TodoWrite,
        "WebFetch" => ToolName::WebFetch,
        "WebSearch" => ToolName::WebSearch,
        "Task" | "AgentTool" => ToolName::Task,
        "NotebookEdit" => ToolName::NotebookEdit,
        "Skill" => ToolName::Skill,
        "StructuredOutput" | "structured_output" => ToolName::StructuredOutput,
        other => ToolName::Other(OtherToolName {
            name: other.to_string(),
        }),
    }
}

pub(super) fn parse_tool_input(name: &ToolName, input: &serde_json::Value) -> ToolInput {
    match name {
        ToolName::Bash => ToolInput::Bash(BashInput {
            command: str_field(input, "command"),
            description: opt_str(input, "description"),
            timeout: input.get("timeout").and_then(|v| v.as_u64()),
            run_in_background: input.get("run_in_background").and_then(|v| v.as_bool()),
        }),
        ToolName::Read => ToolInput::Read(ReadInput {
            file_path: str_field(input, "file_path"),
            offset: input.get("offset").and_then(|v| v.as_u64()),
            limit: input.get("limit").and_then(|v| v.as_u64()),
            pages: opt_str(input, "pages"),
        }),
        ToolName::Edit => ToolInput::Edit(EditInput {
            file_path: str_field(input, "file_path"),
            old_string: str_field(input, "old_string"),
            new_string: str_field(input, "new_string"),
            replace_all: input.get("replace_all").and_then(|v| v.as_bool()),
        }),
        ToolName::Write => ToolInput::Write(WriteInput {
            file_path: str_field(input, "file_path"),
            content: str_field(input, "content"),
        }),
        ToolName::Grep => ToolInput::Grep(GrepInput {
            pattern: str_field(input, "pattern"),
            path: opt_str(input, "path"),
            glob: opt_str(input, "glob"),
            file_type: opt_str(input, "type"),
            output_mode: input
                .get("output_mode")
                .and_then(|v| v.as_str())
                .and_then(parse_grep_output_mode),
            head_limit: input.get("head_limit").and_then(|v| v.as_u64()),
            case_insensitive: input.get("-i").and_then(|v| v.as_bool()),
            multiline: input.get("multiline").and_then(|v| v.as_bool()),
        }),
        ToolName::Glob => ToolInput::Glob(GlobInput {
            pattern: str_field(input, "pattern"),
            path: opt_str(input, "path"),
        }),
        ToolName::TodoWrite => ToolInput::TodoWrite(TodoWriteInput {
            todos: input
                .get("todos")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(parse_todo_item).collect())
                .unwrap_or_default(),
        }),
        ToolName::WebFetch => ToolInput::WebFetch(WebFetchInput {
            url: str_field(input, "url"),
            prompt: str_field(input, "prompt"),
        }),
        ToolName::WebSearch => ToolInput::WebSearch(WebSearchInput {
            query: str_field(input, "query"),
            allowed_domains: opt_string_array(input.get("allowed_domains")),
            blocked_domains: opt_string_array(input.get("blocked_domains")),
        }),
        ToolName::Task => ToolInput::Task(TaskInput {
            description: str_field(input, "description"),
            prompt: str_field(input, "prompt"),
            subagent_type: opt_str(input, "subagent_type"),
        }),
        ToolName::NotebookEdit => ToolInput::NotebookEdit(NotebookEditInput {
            notebook_path: str_field(input, "notebook_path"),
            new_source: str_field(input, "new_source"),
            cell_id: opt_str(input, "cell_id"),
            cell_type: input
                .get("cell_type")
                .and_then(|v| v.as_str())
                .and_then(parse_notebook_cell_type),
            edit_mode: input
                .get("edit_mode")
                .and_then(|v| v.as_str())
                .and_then(parse_notebook_edit_mode),
        }),
        ToolName::Skill => ToolInput::Skill(SkillInput {
            skill: str_field(input, "skill"),
            args: opt_str(input, "args"),
        }),
        ToolName::StructuredOutput => ToolInput::StructuredOutput(StructuredOutputInput {
            raw: serde_json::to_string(input).unwrap_or_default(),
        }),
        ToolName::Mcp(_) | ToolName::Other(_) => ToolInput::Opaque(OpaqueInput {
            raw: serde_json::to_string(input).unwrap_or_default(),
        }),
    }
}

fn parse_grep_output_mode(s: &str) -> Option<GrepOutputMode> {
    match s {
        "content" => Some(GrepOutputMode::Content),
        "files_with_matches" => Some(GrepOutputMode::FilesWithMatches),
        "count" => Some(GrepOutputMode::Count),
        _ => None,
    }
}

fn parse_notebook_cell_type(s: &str) -> Option<NotebookCellType> {
    match s {
        "code" => Some(NotebookCellType::Code),
        "markdown" => Some(NotebookCellType::Markdown),
        _ => None,
    }
}

fn parse_notebook_edit_mode(s: &str) -> Option<NotebookEditMode> {
    match s {
        "replace" => Some(NotebookEditMode::Replace),
        "insert" => Some(NotebookEditMode::Insert),
        "delete" => Some(NotebookEditMode::Delete),
        _ => None,
    }
}

fn parse_todo_item(entry: &serde_json::Value) -> Option<CcTodoItem> {
    let content = entry.get("content").and_then(|v| v.as_str())?.to_string();
    let status = match entry.get("status").and_then(|v| v.as_str())? {
        "pending" => CcTodoItemStatus::Pending,
        "in_progress" | "in-progress" | "active" => CcTodoItemStatus::InProgress,
        "completed" | "done" => CcTodoItemStatus::Completed,
        _ => return None,
    };
    Some(CcTodoItem {
        content,
        active_form: entry
            .get("activeForm")
            .and_then(|v| v.as_str())
            .map(String::from),
        status,
    })
}
