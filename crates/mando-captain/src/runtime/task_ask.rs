//! Task Q&A — multi-turn ask sessions with worktree access.
//!
//! Each task gets a single persistent ask session (stored in `session_ids.ask`).
//! First ask creates a new CC session in the task's worktree; follow-up asks
//! resume the same session via `--resume`.

use anyhow::Result;
use mando_config::workflow::CaptainWorkflow;

use super::dashboard::truncate_utf8;

/// Build the initial prompt for the first ask turn.
///
/// Includes task metadata, timeline context, and the user's question.
/// The session runs in the task's worktree, so it has full code access.
pub fn build_initial_prompt(
    item: &mando_types::Task,
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

    let mut vars = std::collections::HashMap::new();
    vars.insert("title", item.title.as_str());
    vars.insert("id", item_id);
    vars.insert("status", status_str.as_str());
    vars.insert("project", item.project.as_deref().unwrap_or("none"));
    vars.insert("pr", item.pr.as_deref().unwrap_or("none"));
    vars.insert("branch", item.branch.as_deref().unwrap_or("none"));
    vars.insert("context", item.context.as_deref().unwrap_or("none"));
    vars.insert("timeline", timeline_text);
    vars.insert("question", question);

    mando_config::render_prompt("task_analyst", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!(e))
}

/// Build timeline text from recent events.
pub async fn build_timeline_text(pool: &sqlx::SqlitePool, task_id: i64) -> String {
    let events = mando_db::queries::timeline::load_last_n(pool, task_id, 10)
        .await
        .unwrap_or_default();
    if events.is_empty() {
        "No timeline events.".to_string()
    } else {
        events
            .iter()
            .map(|e| format!("[{}] {} — {}", e.timestamp, e.actor, e.summary))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Record ask Q&A in history and timeline.
pub async fn record_ask(pool: &sqlx::SqlitePool, task_id: i64, question: &str, answer: &str) {
    let now = mando_types::now_rfc3339();
    if let Err(e) = mando_db::queries::ask_history::append(
        pool,
        task_id,
        &mando_types::AskHistoryEntry {
            role: "human".into(),
            content: question.into(),
            timestamp: now.clone(),
        },
    )
    .await
    {
        tracing::warn!(task_id, error = %e, "failed to persist ask question to history");
    }
    if let Err(e) = mando_db::queries::ask_history::append(
        pool,
        task_id,
        &mando_types::AskHistoryEntry {
            role: "assistant".into(),
            content: answer.into(),
            timestamp: now,
        },
    )
    .await
    {
        tracing::warn!(task_id, error = %e, "failed to persist ask answer to history");
    }

    super::timeline_emit::emit(
        pool,
        task_id,
        mando_types::timeline::TimelineEventType::HumanAsk,
        "human",
        &format!("Asked: {}", truncate_utf8(question, 80)),
        serde_json::json!({"question": question}),
    )
    .await;
}
