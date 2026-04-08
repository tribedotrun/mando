//! Spawn logic for captain merge sessions.

use std::panic::AssertUnwindSafe;

use anyhow::Result;
use futures::FutureExt;
use rustc_hash::FxHashMap;
use tracing::{info, warn};

use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::task::{ItemStatus, Task};
use mando_types::timeline::TimelineEventType;

use super::captain_merge::merge_json_schema;
use super::notify::Notifier;

/// Spawn a captain merge session for an item. Sets status to CaptainMerging.
pub(crate) async fn spawn_merge(
    item: &mut Task,
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    let cwd = item
        .worktree
        .as_deref()
        .map(std::path::PathBuf::from)
        .or_else(|| {
            config
                .captain
                .projects
                .values()
                .next()
                .map(|p| std::path::PathBuf::from(&p.path))
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no CWD for captain merge: item has no worktree and no projects configured"
            )
        })?;

    let pr_num = item
        .pr_number
        .ok_or_else(|| anyhow::anyhow!("cannot merge item without a PR"))?;

    let pr_number = pr_num.to_string();

    let repo = item
        .github_repo
        .clone()
        .or_else(|| mando_config::resolve_github_repo(Some(&item.project), config))
        .ok_or_else(|| anyhow::anyhow!("no github_repo for project {:?}", item.project))?;

    let pr_url = format!("https://github.com/{repo}/pull/{pr_number}");

    // Render prompt before any side effects so failures propagate as Err
    // rather than dying silently inside tokio::spawn.
    let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
    vars.insert("pr_url", pr_url.as_str());
    vars.insert("repo", repo.as_str());
    vars.insert("pr_number", pr_number.as_str());
    vars.insert("title", item.title.as_str());
    let prompt = mando_config::render_prompt("captain_merge", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!("render captain_merge prompt: {e}"))?;

    item.status = ItemStatus::CaptainMerging;
    item.last_activity_at = Some(mando_types::now_rfc3339());

    let task_id = item.id.to_string();
    let task_id_num = item.id;
    let session_id = mando_uuid::Uuid::v4().to_string();
    item.session_ids.merge = Some(session_id.clone());

    // Persist status + timeline atomically so both survive tick interruption.
    // Items are already CaptainMerging when spawn_merge is called (categorized
    // in poll_merging_items). The guard ensures concurrent ticks don't double-spawn.
    let prev_status = ItemStatus::CaptainMerging;
    let title = mando_shared::telegram_format::escape_html(&item.title);
    let event = mando_types::timeline::TimelineEvent {
        event_type: TimelineEventType::CaptainMergeStarted,
        timestamp: mando_types::now_rfc3339(),
        actor: "captain".to_string(),
        summary: "Captain merge session started".to_string(),
        data: serde_json::json!({ "session_id": &session_id, "pr": &pr_url }),
    };
    match mando_db::queries::tasks::persist_status_transition(
        pool,
        item,
        prev_status.as_str(),
        &event,
    )
    .await
    {
        Ok(true) => {
            notifier
                .normal(&format!(
                    "\u{1f680} Captain merging <b>{title}</b> (<a href=\"{pr_url}\">PR #{pr_number}</a>)"
                ))
                .await;
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
            item.status = prev_status;
            return Err(e);
        }
    }

    let captain_model = workflow.models.captain.clone();
    let timeout = workflow.agent.captain_merge_timeout_s;
    let pool = pool.clone();
    let merge_notifier = notifier.fork();

    let session_id_for_panic = session_id.clone();
    // TRACKED: detached captain-merge CC session. Same rationale as
    // captain_review::spawn_review -- library crate, no AppState dependency,
    // external CC process is managed via the pid registry on shutdown.
    tokio::spawn(async move {
        let result = AssertUnwindSafe(async move {
            let config = mando_cc::CcConfig::builder()
                .model(&captain_model)
                .timeout(timeout)
                .caller("captain-merge-async")
                .task_id(&task_id)
                .cwd(cwd.clone())
                .session_id(session_id.clone())
                .allowed_tools(vec![
                    "Read".into(),
                    "Bash".into(),
                    "Edit".into(),
                    "Write".into(),
                    "Grep".into(),
                    "Glob".into(),
                ])
                .json_schema(merge_json_schema())
                .build();

            // Log "running" session entry so cancel can find it immediately.
            if let Err(e) = crate::io::headless_cc::log_running_session(
                &pool,
                &session_id,
                &cwd,
                "captain-merge-async",
                "",
                Some(task_id_num),
                false,
            )
            .await
            {
                warn!(module = "captain", %session_id, %e, "failed to log running session");
            }

            let sid_for_hook = session_id.clone();
            match mando_cc::CcOneShot::run_with_pid_hook(&prompt, config, |pid| {
            if let Err(e) = crate::io::pid_registry::register(&sid_for_hook, pid) {
                warn!(module = "captain", sid = %sid_for_hook, %e, "pid_registry register failed");
            }
        })
        .await
        {
            Ok(result) => {
                let stream_size = std::fs::metadata(&result.stream_path)
                    .map(|m| m.len())
                    .unwrap_or(u64::MAX);
                info!(
                    module = "captain",
                    %session_id,
                    cost_usd = result.cost_usd.unwrap_or(0.0),
                    duration_ms = result.duration_ms.unwrap_or(0),
                    stream_file_bytes = stream_size,
                    "captain merge CC completed"
                );
                if let Err(e) = crate::io::pid_registry::unregister(&session_id) {
                    warn!(module = "captain", %session_id, %e, "pid_registry unregister failed");
                }
                merge_notifier.check_rate_limit(&result).await;
                if let Err(e) = crate::io::headless_cc::log_cc_result(
                    &pool,
                    &result,
                    &cwd,
                    "captain-merge-async",
                    Some(task_id_num),
                )
                .await {
                    warn!(module = "captain", %session_id, %e, "log_cc_result failed");
                }
            }
            Err(e) => {
                let stream_path = mando_config::stream_path_for_session(&session_id);
                let stream_size = std::fs::metadata(&stream_path)
                    .map(|m| m.len())
                    .unwrap_or(u64::MAX);
                warn!(
                    module = "captain",
                    %session_id,
                    stream_file_bytes = stream_size,
                    error = %e,
                    "captain merge CC failed"
                );
                if let Err(e2) = crate::io::pid_registry::unregister(&session_id) {
                    warn!(module = "captain", %session_id, %e2, "pid_registry unregister failed");
                }
                if let Err(e2) = crate::io::headless_cc::log_cc_failure(
                    &pool,
                    &session_id,
                    &cwd,
                    "captain-merge-async",
                    Some(task_id_num),
                )
                .await {
                    warn!(module = "captain", %session_id, %e2, "log_cc_failure failed");
                }
            }
        }
        })
        .catch_unwind()
        .await;

        if let Err(panic) = result {
            tracing::error!(
                module = "captain",
                session_id = %session_id_for_panic,
                "captain merge spawn panicked: {:?}",
                panic
            );
            let stream_path = mando_config::stream_path_for_session(&session_id_for_panic);
            mando_cc::write_error_result(
                &stream_path,
                &format!("captain merge spawn panicked: {:?}", panic),
            );
        }
    });

    Ok(())
}
