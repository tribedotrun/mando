//! Task Q&A — ask questions about tasks via headless CC.

use anyhow::Result;
use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;

use super::dashboard::truncate_utf8;
use mando_cc::{CcConfig, CcOneShot};

/// Ask a question about a pre-loaded task (avoids holding store lock).
pub async fn ask_task_with(
    config: &Config,
    item: &mando_types::Task,
    item_id: i64,
    pool: &sqlx::SqlitePool,
    question: &str,
    workflow: &CaptainWorkflow,
) -> Result<serde_json::Value> {
    ask_task_with_inner(config, item, &item_id.to_string(), question, workflow, pool).await
}

pub(crate) async fn ask_task_with_inner(
    _config: &Config,
    item: &mando_types::Task,
    item_id: &str,
    question: &str,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
) -> Result<serde_json::Value> {
    // Build context from item fields + recent timeline (from DB).
    let task_id_num: i64 = item_id.parse().unwrap_or(item.id);
    let timeline_events = mando_db::queries::timeline::load_last_n(pool, task_id_num, 10)
        .await
        .unwrap_or_default();
    let timeline_summary: Vec<String> = timeline_events
        .iter()
        .map(|e| format!("[{}] {} — {}", e.timestamp, e.actor, e.summary))
        .collect();

    let status_str = serde_json::to_value(item.status)
        .unwrap_or_default()
        .as_str()
        .unwrap_or("unknown")
        .to_string();
    let timeline_text = if timeline_summary.is_empty() {
        "No timeline events.".to_string()
    } else {
        timeline_summary.join("\n")
    };

    let mut vars = std::collections::HashMap::new();
    vars.insert("title", item.title.as_str());
    vars.insert("id", item_id);
    vars.insert("status", status_str.as_str());
    vars.insert("project", item.project.as_deref().unwrap_or("none"));
    vars.insert("pr", item.pr.as_deref().unwrap_or("none"));
    vars.insert("branch", item.branch.as_deref().unwrap_or("none"));
    vars.insert("context", item.context.as_deref().unwrap_or("none"));
    vars.insert("timeline", timeline_text.as_str());
    vars.insert("question", question);

    let prompt = mando_config::render_prompt("task_analyst", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!(e))?;

    let cc_result = CcOneShot::run(
        &prompt,
        CcConfig::builder()
            .model(&workflow.models.captain)
            .timeout(std::time::Duration::from_secs(60))
            .caller("task-ask")
            .task_id(item_id)
            .build(),
    )
    .await?;

    crate::io::headless_cc::log_cc_session(
        pool,
        &crate::io::headless_cc::SessionLogEntry {
            session_id: &cc_result.session_id,
            cwd: std::path::Path::new(""),
            model: &workflow.models.captain,
            caller: "task-ask",
            cost_usd: cc_result.cost_usd,
            duration_ms: cc_result.duration_ms,
            resumed: false,
            task_id: item_id,
            status: mando_types::SessionStatus::Stopped,
            worker_name: "",
        },
    )
    .await;

    let answer = cc_result.text;

    // Persist Q&A to ask history (DB).
    let now = mando_types::now_rfc3339();
    mando_db::queries::ask_history::append(
        pool,
        task_id_num,
        &mando_types::AskHistoryEntry {
            role: "human".into(),
            content: question.into(),
            timestamp: now.clone(),
        },
    )
    .await?;
    mando_db::queries::ask_history::append(
        pool,
        task_id_num,
        &mando_types::AskHistoryEntry {
            role: "assistant".into(),
            content: answer.clone(),
            timestamp: now,
        },
    )
    .await?;

    // Emit timeline event (DB).
    super::timeline_emit::emit(
        pool,
        task_id_num,
        mando_types::timeline::TimelineEventType::HumanAsk,
        "human",
        &format!("Asked: {}", truncate_utf8(question, 80)),
        serde_json::json!({"question": question}),
    )
    .await;

    Ok(serde_json::json!({
        "id": item_id,
        "question": question,
        "answer": answer,
    }))
}
