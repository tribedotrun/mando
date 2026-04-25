//! Client-side markdown renderer for `mando sessions transcript`.
//!
//! The daemon used to ship a pre-rendered markdown string via
//! `/api/sessions/{id}/transcript`. That route is retained for back-compat,
//! but the CLI now pulls the typed `TranscriptEvent` stream from
//! `/api/sessions/{id}/events` and renders the same-shape markdown locally
//! so piping to files keeps working without bloating the wire payload.

use api_types::{
    AssistantContentBlock, AssistantEvent, AssistantToolUseBlock, CcTodoItemStatus, EditInput,
    ResultEvent, SystemCompactBoundaryEvent, SystemInitEvent, ToolInput, ToolName,
    ToolResultChildBlock, ToolResultContent, TranscriptEvent, UserContentBlock, UserEvent,
    UserToolResultBlock,
};

/// Render a list of typed events to human-readable markdown.
///
/// The shape tracks the legacy `jsonl_to_markdown` output so existing
/// shell scripts that pipe `mando sessions transcript` into files keep
/// working. Retains the `Prompt #N` / `Turn #N` numbering but skips the
/// old "Initial context" dump because that relied on ordering heuristics
/// no longer needed now that we walk structured events directly.
pub fn events_to_markdown(events: &[TranscriptEvent]) -> String {
    let mut out = String::new();
    let mut prompt_num = 0u32;
    let mut turn_num = 0u32;

    for event in events {
        match event {
            TranscriptEvent::SystemInit(init) => render_init(&mut out, init),
            TranscriptEvent::SystemCompactBoundary(b) => render_compact(&mut out, b),
            TranscriptEvent::SystemStatus(_)
            | TranscriptEvent::SystemApiRetry(_)
            | TranscriptEvent::SystemLocalCommandOutput(_)
            | TranscriptEvent::SystemHook(_)
            | TranscriptEvent::SystemRateLimit(_)
            | TranscriptEvent::ToolProgress(_)
            | TranscriptEvent::Unknown(_) => {}
            TranscriptEvent::User(user) => {
                if let Some(md) = render_user(user, &mut prompt_num) {
                    out.push_str(&md);
                }
            }
            TranscriptEvent::Assistant(assistant) => {
                turn_num += 1;
                render_assistant(&mut out, assistant, turn_num);
            }
            TranscriptEvent::Result(result) => render_result(&mut out, result),
        }
    }

    out
}

fn maybe_tick(field: Option<&str>) -> String {
    field.map(|s| format!("  `{s}`")).unwrap_or_default()
}

fn render_init(out: &mut String, init: &SystemInitEvent) {
    let model = init.model.as_deref().filter(|s| !s.is_empty());
    let cwd = init.cwd.as_deref().filter(|s| !s.is_empty());
    let ts = init.meta.timestamp.as_deref().filter(|s| !s.is_empty());
    out.push_str("\n---\n## *Session start*");
    out.push_str(&maybe_tick(model));
    out.push_str(&maybe_tick(cwd));
    out.push_str(&maybe_tick(ts));
    out.push('\n');
}

fn render_compact(out: &mut String, b: &SystemCompactBoundaryEvent) {
    let reason = b.reason.as_deref().unwrap_or("context compacted");
    let ts = b.meta.timestamp.as_deref().filter(|s| !s.is_empty());
    out.push_str(&format!("\n---\n## *Compact boundary — {reason}*"));
    out.push_str(&maybe_tick(ts));
    out.push('\n');
}

fn render_user(user: &UserEvent, prompt_num: &mut u32) -> Option<String> {
    let text_blocks: Vec<&str> = user
        .blocks
        .iter()
        .filter_map(|b| match b {
            UserContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .collect();
    let trimmed = text_blocks.join("\n").trim().to_string();

    let tool_results: Vec<&UserToolResultBlock> = user
        .blocks
        .iter()
        .filter_map(|b| match b {
            UserContentBlock::ToolResult(tr) => Some(tr),
            _ => None,
        })
        .collect();

    if trimmed.is_empty() && tool_results.is_empty() {
        return None;
    }
    if !trimmed.is_empty()
        && (trimmed.contains("<local-command-caveat>")
            || trimmed.contains("<local-command-stdout>"))
    {
        return None;
    }

    let mut out = String::new();
    if !trimmed.is_empty() {
        *prompt_num += 1;
        let ts = user.meta.timestamp.as_deref().filter(|s| !s.is_empty());
        out.push_str(&format!("\n---\n## Prompt #{prompt_num}"));
        out.push_str(&maybe_tick(ts));
        out.push('\n');
        out.push_str(&trimmed);
        out.push('\n');
    }
    for tr in tool_results {
        render_tool_result(&mut out, tr);
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn render_tool_result(out: &mut String, tr: &UserToolResultBlock) {
    let status = if tr.is_error == Some(true) {
        " · error"
    } else {
        ""
    };
    out.push_str(&format!("\n**Tool result** `{}{status}`\n", tr.tool_use_id));
    match &tr.content {
        ToolResultContent::Text(t) => {
            let trimmed = t.text.trim();
            if !trimmed.is_empty() {
                out.push_str("```\n");
                out.push_str(trimmed);
                out.push_str("\n```\n");
            }
        }
        ToolResultContent::Blocks(b) => {
            for child in &b.blocks {
                match child {
                    ToolResultChildBlock::Text(t) => {
                        let trimmed = t.text.trim();
                        if !trimmed.is_empty() {
                            out.push_str("```\n");
                            out.push_str(trimmed);
                            out.push_str("\n```\n");
                        }
                    }
                    ToolResultChildBlock::Image(img) => {
                        let mime = img.media_type.as_deref().unwrap_or("image");
                        out.push_str(&format!("_[image · {mime}]_\n"));
                    }
                    ToolResultChildBlock::Unknown(u) => {
                        out.push_str("```json\n");
                        out.push_str(&u.raw);
                        out.push_str("\n```\n");
                    }
                }
            }
        }
    }
}

fn render_assistant(out: &mut String, evt: &AssistantEvent, turn_num: u32) {
    let ts = evt.meta.timestamp.as_deref().filter(|s| !s.is_empty());
    let model = evt.model.as_deref().filter(|s| !s.is_empty());
    out.push_str(&format!("\n---\n## Turn #{turn_num}"));
    out.push_str(&maybe_tick(model));
    out.push_str(&maybe_tick(ts));
    out.push('\n');

    for block in &evt.blocks {
        match block {
            AssistantContentBlock::Text(t) => {
                let trimmed = t.text.trim();
                if !trimmed.is_empty() {
                    out.push_str(trimmed);
                    out.push('\n');
                }
            }
            AssistantContentBlock::Thinking(t) => {
                let trimmed = t.text.trim();
                if !trimmed.is_empty() {
                    out.push_str(&format!("\n> _(thinking)_ {trimmed}\n"));
                }
            }
            AssistantContentBlock::ToolUse(tu) => render_tool_use(out, tu),
        }
    }
}

fn render_tool_use(out: &mut String, tu: &AssistantToolUseBlock) {
    let name = tool_name_label(&tu.name);
    out.push_str(&format!("\n**{name}**\n"));
    match &tu.input {
        ToolInput::Bash(b) => {
            if let Some(desc) = b.description.as_deref().filter(|d| !d.is_empty()) {
                out.push_str(&format!("_{desc}_\n"));
            }
            out.push_str(&format!("```bash\n{}\n```\n", b.command));
        }
        ToolInput::Read(r) => {
            let mut line = format!("`{}`", r.file_path);
            if let (Some(o), Some(l)) = (r.offset, r.limit) {
                line.push_str(&format!(" · lines {o}..{}", o.saturating_add(l)));
            } else if let Some(l) = r.limit {
                line.push_str(&format!(" · first {l} lines"));
            } else if let Some(o) = r.offset {
                line.push_str(&format!(" · from line {o}"));
            }
            if let Some(pages) = r.pages.as_deref().filter(|p| !p.is_empty()) {
                line.push_str(&format!(" · pages {pages}"));
            }
            out.push_str(&line);
            out.push('\n');
        }
        ToolInput::Edit(e) => render_edit(out, e),
        ToolInput::Write(w) => {
            let line_count = w.content.lines().count();
            out.push_str(&format!("`{}` · {line_count} line(s)\n", w.file_path));
        }
        ToolInput::Grep(g) => {
            let mut line = format!("`{}`", g.pattern);
            if let Some(path) = g.path.as_deref() {
                line.push_str(&format!(" in `{path}`"));
            }
            if let Some(glob) = g.glob.as_deref() {
                line.push_str(&format!(" glob `{glob}`"));
            }
            out.push_str(&line);
            out.push('\n');
        }
        ToolInput::Glob(g) => {
            let mut line = format!("`{}`", g.pattern);
            if let Some(path) = g.path.as_deref() {
                line.push_str(&format!(" in `{path}`"));
            }
            out.push_str(&line);
            out.push('\n');
        }
        ToolInput::TodoWrite(t) => {
            for item in &t.todos {
                let marker = match item.status {
                    CcTodoItemStatus::Pending => "- [ ]",
                    CcTodoItemStatus::InProgress => "- [~]",
                    CcTodoItemStatus::Completed => "- [x]",
                };
                out.push_str(&format!("{marker} {}\n", item.content));
            }
        }
        ToolInput::WebFetch(w) => {
            out.push_str(&format!("`{}`\n\n{}\n", w.url, w.prompt));
        }
        ToolInput::WebSearch(w) => {
            out.push_str(&format!("`{}`\n", w.query));
        }
        ToolInput::Task(t) => {
            let mut line = format!("_{}_", t.description);
            if let Some(agent) = t.subagent_type.as_deref().filter(|a| !a.is_empty()) {
                line.push_str(&format!("  `{agent}`"));
            }
            out.push_str(&line);
            out.push('\n');
            out.push_str(&t.prompt);
            out.push('\n');
        }
        ToolInput::NotebookEdit(n) => {
            out.push_str(&format!("`{}`\n", n.notebook_path));
        }
        ToolInput::Skill(s) => {
            let args = s.args.as_deref().unwrap_or("");
            out.push_str(&format!("`/{} {}`\n", s.skill, args));
        }
        ToolInput::StructuredOutput(s) => {
            out.push_str(&format!("```json\n{}\n```\n", s.raw));
        }
        ToolInput::Opaque(o) => {
            out.push_str(&format!("```json\n{}\n```\n", o.raw));
        }
    }
}

fn render_edit(out: &mut String, e: &EditInput) {
    out.push_str(&format!("`{}`\n", e.file_path));
    out.push_str("```diff\n");
    for line in e.old_string.lines() {
        out.push_str(&format!("- {line}\n"));
    }
    for line in e.new_string.lines() {
        out.push_str(&format!("+ {line}\n"));
    }
    out.push_str("```\n");
}

fn render_result(out: &mut String, r: &ResultEvent) {
    let mut footer = String::from("\n---\n## *Result*");
    let status = if r.summary.is_error { "error" } else { "ok" };
    footer.push_str(&format!("  `{status}`"));
    if let Some(cost) = r.summary.total_cost_usd {
        footer.push_str(&format!("  `${cost:.4}`"));
    }
    if let Some(turns) = r.summary.num_turns {
        footer.push_str(&format!("  `{turns} turns`"));
    }
    if let Some(stop) = r.summary.stop_reason.as_deref() {
        footer.push_str(&format!("  `{stop}`"));
    }
    footer.push('\n');
    out.push_str(&footer);
    for err in &r.summary.errors {
        out.push_str(&format!("**Error:** {err}\n"));
    }
}

fn tool_name_label(name: &ToolName) -> String {
    match name {
        ToolName::Bash => "Bash".into(),
        ToolName::Read => "Read".into(),
        ToolName::Edit => "Edit".into(),
        ToolName::Write => "Write".into(),
        ToolName::Grep => "Grep".into(),
        ToolName::Glob => "Glob".into(),
        ToolName::TodoWrite => "TodoWrite".into(),
        ToolName::WebFetch => "WebFetch".into(),
        ToolName::WebSearch => "WebSearch".into(),
        ToolName::Task => "Task".into(),
        ToolName::NotebookEdit => "NotebookEdit".into(),
        ToolName::Skill => "Skill".into(),
        ToolName::StructuredOutput => "StructuredOutput".into(),
        ToolName::Mcp(m) => format!("MCP {}/{}", m.server, m.tool),
        ToolName::Other(o) => o.name.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_types::{
        AssistantTextBlock, BashInput, EventIndex, EventMeta, ResultOutcome, ResultSummary,
        UserTextBlock,
    };

    fn meta() -> EventMeta {
        EventMeta {
            index: EventIndex { line_number: 1 },
            uuid: None,
            parent_uuid: None,
            session_id: None,
            timestamp: Some("2026-04-24T00:00:00Z".into()),
            is_sidechain: None,
        }
    }

    #[test]
    fn renders_prompt_and_turn_numbering() {
        let events = vec![
            TranscriptEvent::User(UserEvent {
                meta: meta(),
                blocks: vec![UserContentBlock::Text(UserTextBlock {
                    text: "hello".into(),
                })],
            }),
            TranscriptEvent::Assistant(AssistantEvent {
                meta: meta(),
                model: Some("claude-haiku".into()),
                blocks: vec![AssistantContentBlock::Text(AssistantTextBlock {
                    text: "hi".into(),
                })],
                usage: None,
                stop_reason: None,
            }),
        ];
        let md = events_to_markdown(&events);
        assert!(md.contains("## Prompt #1"));
        assert!(md.contains("hello"));
        assert!(md.contains("## Turn #1"));
        assert!(md.contains("hi"));
    }

    #[test]
    fn renders_bash_tool_use() {
        let events = vec![TranscriptEvent::Assistant(AssistantEvent {
            meta: meta(),
            model: None,
            blocks: vec![AssistantContentBlock::ToolUse(AssistantToolUseBlock {
                id: "tu".into(),
                name: ToolName::Bash,
                input: ToolInput::Bash(BashInput {
                    command: "ls".into(),
                    description: Some("list".into()),
                    timeout: None,
                    run_in_background: None,
                }),
            })],
            usage: None,
            stop_reason: None,
        })];
        let md = events_to_markdown(&events);
        assert!(md.contains("**Bash**"));
        assert!(md.contains("_list_"));
        assert!(md.contains("```bash\nls\n```"));
    }

    #[test]
    fn renders_result_footer() {
        let events = vec![TranscriptEvent::Result(ResultEvent {
            meta: meta(),
            outcome: ResultOutcome::Success,
            summary: ResultSummary {
                duration_ms: Some(100),
                duration_api_ms: None,
                num_turns: Some(3),
                total_cost_usd: Some(0.01),
                stop_reason: Some("end_turn".into()),
                permission_denials: Vec::new(),
                errors: Vec::new(),
                usage: None,
                model_usage: Vec::new(),
                is_error: false,
            },
        })];
        let md = events_to_markdown(&events);
        assert!(md.contains("## *Result*"));
        assert!(md.contains("`ok`"));
        assert!(md.contains("3 turns"));
    }
}
