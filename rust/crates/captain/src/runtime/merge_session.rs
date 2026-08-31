//! Provider-neutral captain merge phase runner.

use anyhow::Result;
use global_types::TaskOwnerProvider;
use settings::CaptainWorkflow;

use crate::io::session_terminate;
use crate::service::lifecycle;
use crate::{ItemStatus, Task};

use super::captain_merge::{merge_json_schema, merge_started_event, notify_merge_started};
use super::claude_detached_session::DetachedClaudeSession;
use super::notify::Notifier;

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all, fields(provider = %item.provider.as_str(), task_id = item.id, pr_number))]
pub(super) async fn spawn(
    item: &mut Task,
    cwd: &std::path::Path,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
    pr_url: &str,
    pr_number: &str,
    prompt: &str,
    workflow: &CaptainWorkflow,
) -> Result<()> {
    let owner = super::agent_runtime::Adapter::for_task(item)?.task_owner()?;
    match owner {
        TaskOwnerProvider::Claude => {
            spawn_claude(
                item, cwd, notifier, pool, pr_url, pr_number, prompt, workflow,
            )
            .await
        }
        TaskOwnerProvider::Codex => {
            spawn_codex(
                item, cwd, notifier, pool, pr_url, pr_number, prompt, workflow,
            )
            .await
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn spawn_claude(
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

    let event = merge_started_event(&session_id, pr_url);
    match crate::io::queries::tasks::persist_merge_spawn(pool, item, &event).await {
        Ok(true) => notify_merge_started(notifier, item, pr_url, pr_number).await,
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
            tracing::error!(module = "captain", item_id = item.id, error = %e, "failed to persist merge spawn -- skipping, will retry next tick");
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

#[allow(clippy::too_many_arguments)]
async fn spawn_codex(
    item: &mut Task,
    cwd: &std::path::Path,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
    pr_url: &str,
    pr_number: &str,
    prompt: &str,
    workflow: &CaptainWorkflow,
) -> Result<()> {
    let started = super::agent_runtime::spawn_structured_session(
        item.provider,
        pool,
        "captain-merge-async",
        item.id,
        &item.project,
        "",
        cwd,
        prompt,
        super::agent_runtime::AgentOutputSchema(merge_json_schema()),
        None,
        &workflow.agent,
    )
    .await?;

    item.last_activity_at = Some(global_types::now_rfc3339());
    item.session_ids.merge = Some(started.session_id.clone());
    let event = merge_started_event(&started.session_id, pr_url);
    match crate::io::queries::tasks::persist_merge_spawn(pool, item, &event).await {
        Ok(true) => {
            notify_merge_started(notifier, item, pr_url, pr_number).await;
            tracing::info!(module = "captain", session_id = %started.session_id, "codex merge session spawned");
            Ok(())
        }
        Ok(false) => {
            session_terminate::terminate_session(
                pool,
                &started.session_id,
                global_types::SessionStatus::Stopped,
                None,
            )
            .await;
            item.session_ids.merge = None;
            tracing::info!(
                module = "captain",
                item_id = item.id,
                "merge spawn already applied"
            );
            Ok(())
        }
        Err(e) => {
            session_terminate::terminate_session(
                pool,
                &started.session_id,
                global_types::SessionStatus::Stopped,
                None,
            )
            .await;
            item.session_ids.merge = None;
            lifecycle::restore_status(item, ItemStatus::CaptainMerging);
            Err(e)
        }
    }
}
