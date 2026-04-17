//! Task Q&A — multi-turn ask sessions with worktree access.
//!
//! Each task gets a single persistent ask session (stored in `session_ids.ask`).
//! First ask creates a new CC session in the task's worktree; follow-up asks
//! resume the same session via `--resume`.

use anyhow::Result;
use rustc_hash::FxHashMap;
use settings::config::workflow::CaptainWorkflow;

use super::dashboard::truncate_utf8;

/// Build the initial prompt for the first ask turn.
///
/// Includes task metadata, timeline context, and the user's question.
/// The session runs in the task's worktree, so it has full code access.
pub fn build_initial_prompt(
    item: &crate::Task,
    item_id: &str,
    question: &str,
    workflow: &CaptainWorkflow,
    timeline_text: &str,
) -> Result<String> {
    let status_str = serde_json::to_value(item.status)
        .unwrap_or_default()
        .as_str()
        .unwrap_or("unknown")
        .to_string();

    let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
    vars.insert("title", item.title.as_str());
    vars.insert("id", item_id);
    vars.insert("status", status_str.as_str());
    vars.insert(
        "project",
        if item.project.is_empty() {
            "none"
        } else {
            &item.project
        },
    );
    let pr_str = item
        .pr_number
        .map(|n| n.to_string())
        .unwrap_or_else(|| "none".to_string());
    vars.insert("pr", &pr_str);
    vars.insert("branch", item.branch.as_deref().unwrap_or("none"));
    vars.insert("context", item.context.as_deref().unwrap_or("none"));
    vars.insert("timeline", timeline_text);
    vars.insert("question", question);

    settings::config::render_prompt("task_ask", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!(e))
}

/// Build timeline text from recent events. DB errors are propagated so the
/// caller surfaces "we couldn't read your history" instead of silently
/// handing the worker an empty timeline (which looks like a fresh task).
pub async fn build_timeline_text(pool: &sqlx::SqlitePool, task_id: i64) -> Result<String> {
    let events = crate::io::queries::timeline::load_last_n(pool, task_id, 10)
        .await
        .map_err(|e| anyhow::anyhow!("load_last_n({task_id}): {e}"))?;
    if events.is_empty() {
        Ok("No timeline events.".to_string())
    } else {
        Ok(events
            .iter()
            .map(|e| format!("[{}] {} — {}", e.timestamp, e.actor, e.summary))
            .collect::<Vec<_>>()
            .join("\n"))
    }
}

/// Persist a human question to ask_history. Called immediately on receipt,
/// before the CC session runs, so the question survives timeouts/crashes.
pub async fn persist_question(
    pool: &sqlx::SqlitePool,
    task_id: i64,
    ask_id: &str,
    session_id: &str,
    question: &str,
) -> Result<()> {
    crate::io::queries::ask_history::append(
        pool,
        task_id,
        &crate::AskHistoryEntry {
            ask_id: ask_id.into(),
            session_id: session_id.into(),
            role: "human".into(),
            content: question.into(),
            timestamp: global_types::now_rfc3339(),
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!("persist ask question for task {task_id}: {e}"))
}

/// Persist the assistant answer and emit a HumanAsk timeline event.
pub async fn persist_answer(
    pool: &sqlx::SqlitePool,
    task_id: i64,
    ask_id: &str,
    session_id: &str,
    question: &str,
    answer: &str,
    intent: &str,
) -> Result<()> {
    crate::io::queries::ask_history::append(
        pool,
        task_id,
        &crate::AskHistoryEntry {
            ask_id: ask_id.into(),
            session_id: session_id.into(),
            role: "assistant".into(),
            content: answer.into(),
            timestamp: global_types::now_rfc3339(),
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!("persist ask answer for task {task_id}: {e}"))?;

    let prefix = match intent {
        "reopen" => "Reopen request",
        "rework" => "Rework request",
        _ => "Asked",
    };

    super::timeline_emit::emit(
        pool,
        task_id,
        crate::TimelineEventType::HumanAsk,
        "human",
        &format!("{prefix}: {}", truncate_utf8(question, 80)),
        serde_json::json!({"question": question, "intent": intent, "ask_id": ask_id}),
    )
    .await
}

/// Persist an error to ask_history and emit a HumanAskFailed timeline event.
pub async fn persist_error(
    pool: &sqlx::SqlitePool,
    task_id: i64,
    ask_id: &str,
    session_id: &str,
    question: &str,
    error_msg: &str,
) -> Result<()> {
    crate::io::queries::ask_history::append(
        pool,
        task_id,
        &crate::AskHistoryEntry {
            ask_id: ask_id.into(),
            session_id: session_id.into(),
            role: "error".into(),
            content: error_msg.into(),
            timestamp: global_types::now_rfc3339(),
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!("persist ask error for task {task_id}: {e}"))?;

    super::timeline_emit::emit(
        pool,
        task_id,
        crate::TimelineEventType::HumanAskFailed,
        "system",
        &format!("Ask failed: {}", truncate_utf8(error_msg, 80)),
        serde_json::json!({"question": question, "error": error_msg}),
    )
    .await
}
