//! Task Q&A — multi-turn ask sessions with worktree access.
//!
//! Each task gets a single persistent ask session (stored in `session_ids.ask`).
//! First ask creates a new CC session in the task's worktree; follow-up asks
//! resume the same session via `--resume`.

use anyhow::Result;
use mando_config::workflow::CaptainWorkflow;
use rustc_hash::FxHashMap;

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

    mando_config::render_prompt("task_ask", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!(e))
}

/// Build timeline text from recent events. DB errors are propagated so the
/// caller surfaces "we couldn't read your history" instead of silently
/// handing the worker an empty timeline (which looks like a fresh task).
pub async fn build_timeline_text(pool: &sqlx::SqlitePool, task_id: i64) -> Result<String> {
    let events = mando_db::queries::timeline::load_last_n(pool, task_id, 10)
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

/// Record ask Q&A in history and timeline. Any persistence failure is
/// propagated so the caller returns 500 to the human instead of silently
/// leaving the DB inconsistent (history/timeline out of sync with the CC
/// session).
pub async fn record_ask(
    pool: &sqlx::SqlitePool,
    task_id: i64,
    question: &str,
    answer: &str,
) -> Result<()> {
    let now = mando_types::now_rfc3339();
    mando_db::queries::ask_history::append(
        pool,
        task_id,
        &mando_types::AskHistoryEntry {
            role: "human".into(),
            content: question.into(),
            timestamp: now.clone(),
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!("persist ask question for task {task_id}: {e}"))?;

    mando_db::queries::ask_history::append(
        pool,
        task_id,
        &mando_types::AskHistoryEntry {
            role: "assistant".into(),
            content: answer.into(),
            timestamp: now,
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!("persist ask answer for task {task_id}: {e}"))?;

    super::timeline_emit::emit(
        pool,
        task_id,
        mando_types::timeline::TimelineEventType::HumanAsk,
        "human",
        &format!("Asked: {}", truncate_utf8(question, 80)),
        serde_json::json!({"question": question}),
    )
    .await?;
    Ok(())
}
