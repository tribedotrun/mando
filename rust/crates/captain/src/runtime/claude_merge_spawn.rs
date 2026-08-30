//! Claude adapter for captain merge sessions.

use anyhow::Result;
use settings::CaptainWorkflow;

use crate::Task;

use super::captain_merge::{merge_json_schema, merge_started_event, notify_merge_started};
use super::claude_detached_session::DetachedClaudeSession;
use super::notify::Notifier;

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all, fields(provider = "claude", task_id = item.id, pr_number))]
pub(super) async fn spawn_claude_merge(
    item: &mut Task,
    cwd: &std::path::Path,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
    pr_url: &str,
    pr_number: &str,
    prompt: &str,
    workflow: &CaptainWorkflow,
) -> Result<()> {
    item.last_activity_at = Some(global_types::now_rfc3339());

    let session_id = global_infra::uuid::Uuid::v4().to_string();
    item.session_ids.merge = Some(session_id.clone());

    // Persist status + timeline atomically so both survive tick interruption.
    // Items are already CaptainMerging when spawn_merge is called (categorized
    // in poll_merging_items). The guard ensures concurrent ticks don't double-spawn.
    let event = merge_started_event(&session_id, pr_url);
    match crate::io::queries::tasks::persist_merge_spawn(pool, item, &event).await {
        Ok(true) => {
            notify_merge_started(notifier, item, pr_url, pr_number).await;
        }
        Ok(false) => {
            tracing::info!(
                module = "captain",
                item_id = item.id,
                "merge spawn already applied"
            );
            item.session_ids.merge = None;
            return Ok(());
        }
        Err(e) => {
            tracing::error!(module = "captain", item_id = item.id, error = %e,
                "failed to persist merge spawn -- skipping, will retry next tick");
            item.session_ids.merge = None;
            return Err(e);
        }
    }

    let credential = super::tick_spawn::pick_credential(pool).await;

    super::claude_detached_session::spawn_detached_claude_session(DetachedClaudeSession {
        caller: "captain-merge-async",
        phase: "captain merge",
        session_id,
        task_id: item.id,
        cwd: cwd.to_path_buf(),
        prompt: prompt.to_string(),
        model: workflow.models.captain.clone(),
        timeout: workflow.agent.captain_merge_timeout_s,
        cc_max_retries: workflow.agent.cc_max_retries,
        effort: workflow.agent.cc_effort,
        allowed_tools: vec![
            "Read".into(),
            "Bash".into(),
            "Edit".into(),
            "Write".into(),
            "Grep".into(),
            "Glob".into(),
        ],
        disallowed_tools: Vec::new(),
        json_schema: merge_json_schema(),
        slot: crate::SessionSlot::Merge,
        credential,
        notifier: notifier.fork(),
        pool: pool.clone(),
    })
    .await;

    Ok(())
}
