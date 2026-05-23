use std::path::Path;

use anyhow::Result;
use settings::CaptainWorkflow;
use tracing::info;

use crate::io::session_terminate;
use crate::service::lifecycle;
use crate::{ItemStatus, Task, TimelineEventPayload};

use super::captain_merge::merge_json_schema;
use super::notify::Notifier;

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip(item, cwd, notifier, pool, prompt), fields(task_id = item.id, provider = "codex"))]
pub(super) async fn spawn_codex_merge(
    item: &mut Task,
    cwd: &Path,
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
    let event = crate::TimelineEvent {
        timestamp: global_types::now_rfc3339(),
        actor: "captain".to_string(),
        summary: "Captain merge session started".to_string(),
        data: TimelineEventPayload::CaptainMergeStarted {
            session_id: started.session_id.clone(),
            pr: pr_url.to_string(),
        },
    };
    match crate::io::queries::tasks::persist_status_transition(
        pool,
        item,
        ItemStatus::CaptainMerging.as_str(),
        &event,
    )
    .await
    {
        Ok(true) => {
            let title = global_infra::html::escape_html(&item.title);
            notifier
                .normal(&format!(
                    "🚀 Captain merging <b>{title}</b> (<a href=\"{pr_url}\">PR #{pr_number}</a>)"
                ))
                .await;
            info!(module = "captain", session_id = %started.session_id, "codex merge session spawned");
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
