//! Async, non-blocking captain merge sessions.
//!
//! When an item enters CaptainMerging, the captain:
//! 1. Spawns a headless CC session with merge instructions
//! 2. The session checks CI, triggers it if needed, fixes failures, and merges
//! 3. On subsequent ticks, polls for completion
//! 4. Applies the result (merged or escalate)

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::task::{ItemStatus, Task};
use mando_types::timeline::TimelineEventType;

use super::notify::Notifier;
use super::timeline_emit;
use crate::io::process_manager;

/// Structured result from a captain merge CC session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeResult {
    pub action: String,
    pub feedback: String,
}

/// JSON Schema for the MergeResult structured output.
fn merge_json_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["merged", "escalate"],
                "description": "merged = PR successfully merged; escalate = could not merge, needs human"
            },
            "feedback": {
                "type": "string",
                "description": "Summary of what was done or why escalation is needed"
            }
        },
        "required": ["action", "feedback"]
    })
}

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

    let pr_url = item
        .pr
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("cannot merge item without a PR"))?
        .to_string();

    let (repo, pr_number) = super::ci_gate::parse_pr_url(&pr_url)
        .ok_or_else(|| anyhow::anyhow!("cannot parse PR URL: {}", pr_url))?;

    item.status = ItemStatus::CaptainMerging;
    item.last_activity_at = Some(mando_types::now_rfc3339());

    let task_id = item.best_id();
    let session_id = mando_uuid::Uuid::v4().to_string();
    item.session_ids.merge = Some(session_id.clone());

    let title = mando_shared::telegram_format::escape_html(&item.title);
    timeline_emit::emit_for_task(
        item,
        TimelineEventType::CaptainMergeStarted,
        "Captain merge session started",
        serde_json::json!({ "session_id": &session_id, "pr": &pr_url }),
        pool,
    )
    .await;
    notifier
        .normal(&format!(
            "\u{1f680} Captain merging <b>{title}</b> (<a href=\"{pr_url}\">PR #{pr_number}</a>)"
        ))
        .await;

    let captain_model = workflow.models.captain.clone();
    let timeout_s = workflow.agent.captain_merge_timeout_s;
    let prompts = workflow.prompts.clone();
    let pool = pool.clone();
    let merge_notifier = notifier.fork();
    let item_title = item.title.clone();

    tokio::spawn(async move {
        let mut vars = std::collections::HashMap::new();
        vars.insert("pr_url", pr_url.as_str());
        vars.insert("repo", repo.as_str());
        vars.insert("pr_number", pr_number.as_str());
        vars.insert("title", item_title.as_str());

        let prompt = match mando_config::render_prompt("captain_merge", &prompts, &vars) {
            Ok(p) => p,
            Err(e) => {
                warn!(module = "captain", %session_id, %e, "failed to render captain_merge prompt");
                return;
            }
        };

        let config = mando_cc::CcConfig::builder()
            .model(&captain_model)
            .timeout(std::time::Duration::from_secs(timeout_s))
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

        match mando_cc::CcOneShot::run(&prompt, config).await {
            Ok(result) => {
                info!(module = "captain", %session_id, "captain merge CC completed");
                merge_notifier.check_rate_limit(&result).await;
                crate::io::headless_cc::log_cc_result(
                    &pool,
                    &result,
                    &cwd,
                    "captain-merge-async",
                    &task_id,
                )
                .await;
            }
            Err(e) => {
                warn!(module = "captain", %session_id, %e, "captain merge CC failed");
                crate::io::headless_cc::log_cc_failure(
                    &pool,
                    &session_id,
                    &cwd,
                    "captain-merge-async",
                    &task_id,
                )
                .await;
            }
        }
    });

    Ok(())
}

/// Check if a captain merge session has completed. Returns the result if done.
pub(crate) fn check_merge(item: &Task) -> Option<MergeResult> {
    let session_id = item.session_ids.merge.as_deref()?;
    let stream_path = mando_config::stream_path_for_session(session_id);
    let result = mando_cc::get_stream_result(&stream_path)?;

    // Try structured_output first.
    if let Some(so) = result.get("structured_output").filter(|v| !v.is_null()) {
        match serde_json::from_value::<MergeResult>(so.clone()) {
            Ok(mr) => return Some(mr),
            Err(e) => {
                warn!(module = "captain", %e, %session_id, "merge structured_output parse failed");
            }
        }
    }

    // Fall back to result text.
    let mut text = result
        .get("result")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if text.is_empty() {
        if let Some(t) = process_manager::get_last_assistant_text(&stream_path) {
            text = t;
        } else {
            return Some(MergeResult {
                action: "escalate".into(),
                feedback: "Merge session completed but produced no output".into(),
            });
        }
    }

    match serde_json::from_str::<MergeResult>(&text) {
        Ok(mr) => Some(mr),
        Err(e) => {
            warn!(module = "captain", %e, "failed to parse merge result, escalating");
            Some(MergeResult {
                action: "escalate".into(),
                feedback: format!("Failed to parse merge result: {e}"),
            })
        }
    }
}

/// Apply a merge session result to an item.
pub(crate) async fn apply_merge_result(
    item: &mut Task,
    result: &MergeResult,
    notifier: &Notifier,
    config: &Config,
    pool: &sqlx::SqlitePool,
) {
    item.session_ids.merge = None;
    let title = mando_shared::telegram_format::escape_html(&item.title);
    let data = serde_json::json!({ "action": result.action, "feedback": result.feedback });

    match result.action.as_str() {
        "merged" => {
            item.status = ItemStatus::Merged;
            timeline_emit::emit_for_task(
                item,
                TimelineEventType::Merged,
                &format!("Captain merged: {}", result.feedback),
                data,
                pool,
            )
            .await;
            notifier
                .high(&format!("\u{1f389} Captain merged <b>{title}</b>"))
                .await;

            if let Err(e) = super::linear_integration::writeback_status(item, config).await {
                tracing::warn!(module = "captain", %e, "Linear status writeback failed");
            }
        }
        _ => {
            // escalate or unknown → Escalated
            item.status = ItemStatus::Escalated;
            item.escalation_report = Some(result.feedback.clone());
            timeline_emit::emit_for_task(
                item,
                TimelineEventType::Escalated,
                &format!("Merge escalated: {}", result.feedback),
                data,
                pool,
            )
            .await;
            notifier
                .critical(&format!(
                    "\u{1f6a8} Merge escalated <b>{title}</b>: {}",
                    mando_shared::telegram_format::escape_html(&result.feedback),
                ))
                .await;
        }
    }
}

/// Poll all CaptainMerging items — spawn sessions, check results, handle timeouts.
pub(crate) async fn poll_merging_items(
    items: &mut [Task],
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) {
    let merge_timeout_s = workflow.agent.captain_merge_timeout_s;
    let max_merge_retries = workflow.agent.max_review_retries;
    for item in items
        .iter_mut()
        .filter(|it| it.status == ItemStatus::CaptainMerging)
    {
        let has_session = item
            .session_ids
            .merge
            .as_deref()
            .is_some_and(|s| !s.is_empty());
        if !has_session {
            item.last_activity_at = Some(mando_types::now_rfc3339());
            if let Err(e) = spawn_merge(item, config, workflow, notifier, pool).await {
                tracing::warn!(module = "captain", item_id = item.id, error = %e, "spawn_merge failed");
                handle_merge_error(
                    item,
                    &format!("spawn failed: {e}"),
                    max_merge_retries,
                    notifier,
                    pool,
                )
                .await;
            }
            continue;
        }

        if let Some(result) = check_merge(item) {
            apply_merge_result(item, &result, notifier, config, pool).await;
        } else {
            let is_timed_out = item
                .last_activity_at
                .as_deref()
                .and_then(|ts| {
                    time::OffsetDateTime::parse(ts, &time::format_description::well_known::Rfc3339)
                        .ok()
                })
                .map(|entered| {
                    let elapsed = time::OffsetDateTime::now_utc() - entered;
                    elapsed.whole_seconds() as u64 > merge_timeout_s
                })
                .unwrap_or(true);

            if is_timed_out {
                handle_merge_error(
                    item,
                    "merge session timed out without producing a result",
                    max_merge_retries,
                    notifier,
                    pool,
                )
                .await;
            }
        }
    }
}

/// Handle merge session error (CC crashed/timed out).
///
/// Retries up to `max_review_retries` before escalating — transient failures
/// (GitHub API blips, CC timeouts) are common during merge operations.
pub(crate) async fn handle_merge_error(
    item: &mut Task,
    error: &str,
    max_retries: u32,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) {
    item.session_ids.merge = None;
    item.retry_count += 1;
    let fail_count = item.retry_count as u32;
    let title = mando_shared::telegram_format::escape_html(&item.title);
    let err_data = serde_json::json!({ "error": error, "fail_count": fail_count });

    if fail_count >= max_retries {
        item.status = ItemStatus::Escalated;
        item.escalation_report = Some(format!("Merge failed {fail_count}/{max_retries}: {error}"));
        timeline_emit::emit_for_task(
            item,
            TimelineEventType::Escalated,
            &format!("Merge failed {fail_count}/{max_retries}: {error}"),
            err_data,
            pool,
        )
        .await;
        let escaped_error = mando_shared::telegram_format::escape_html(error);
        notifier
            .critical(&format!(
                "\u{1f6a8} Merge failed for <b>{title}</b>: {escaped_error}"
            ))
            .await;
    } else {
        // Stay in CaptainMerging — will retry on next tick.
        tracing::warn!(module = "captain", fail_count, max = max_retries, %error,
            "merge session failed, will retry");
        timeline_emit::emit_for_task(
            item,
            TimelineEventType::CaptainMergeStarted,
            &format!("Merge attempt {fail_count}/{max_retries} failed: {error}"),
            err_data,
            pool,
        )
        .await;
    }
}
