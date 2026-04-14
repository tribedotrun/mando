//! Async, non-blocking captain review sessions.
//!
//! When the classifier decides an item needs CC review, the captain:
//! 1. Spawns a headless CC session (non-blocking)
//! 2. Sets item status to CaptainReviewing
//! 3. On subsequent ticks, polls for completion
//! 4. Applies the verdict (ship/nudge/respawn/escalate/retry)

use std::panic::AssertUnwindSafe;

use anyhow::Result;
use futures::FutureExt;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::task::{ItemStatus, Task};
use mando_types::timeline::TimelineEventType;

use super::notify::Notifier;
use super::review_phase;

#[cfg(test)]
pub(crate) use super::captain_review_check::validate_verdict;
pub(crate) use super::captain_review_check::{check_review, check_review_failed};
pub use super::captain_review_error::handle_review_error;
use super::captain_review_helpers::escaped_title;
pub use super::captain_review_verdict::apply_verdict;

/// Structured verdict from a captain review CC session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptainVerdict {
    pub action: String,
    pub feedback: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<String>,
}

/// All recognized captain review triggers; used to populate `is_<trigger>`
/// template variables for the captain review prompt.
const TRIGGERS: &[&str] = &[
    "gates_pass",
    "degraded_context",
    "timeout",
    "broken_session",
    "budget_exhausted",
    "clarifier_fail",
    "rebase_fail",
    "ci_failure",
    "merge_fail",
    "repeated_nudge",
];

/// Allowed actions for a given trigger, matching `is_verdict_allowed`.
fn allowed_actions_for_trigger(trigger: &str) -> &'static [&'static str] {
    match trigger {
        "clarifier_fail" => &["retry_clarifier", "escalate"],
        "budget_exhausted" => &["ship", "nudge", "respawn", "reset_budget", "escalate"],
        _ => &["ship", "nudge", "respawn", "reset_budget"],
    }
}

/// JSON Schema for the CaptainVerdict structured output.
/// Trigger-aware: only offers actions the captain can actually choose.
fn verdict_json_schema(trigger: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": allowed_actions_for_trigger(trigger),
                "description": "The verdict action — must be one of the allowed values"
            },
            "feedback": {
                "type": "string",
                "description": "Specific instructions for worker or summary for human"
            },
            "report": {
                "type": "string",
                "description": "CTO-level report, required for escalate"
            }
        },
        "required": ["action", "feedback"]
    })
}

/// Spawn a captain review for an item. Sets status to CaptainReviewing.
///
/// The CC session runs async (tokio::spawn) — not awaited here.
/// `db_status` is the status the DB currently has for this task. When callers
/// use `reset_review_retry` before `spawn_review`, they must pass the
/// pre-reset status so the atomic persist guard matches the DB row.
/// When the item is already `CaptainReviewing` in the DB (e.g., tick_review
/// no-session path), pass `None` to use the item's current status.
pub(crate) async fn spawn_review(
    item: &mut Task,
    trigger: &str,
    db_status: Option<&str>,
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    // Resolve CWD before mutating item state — if this fails,
    // the item stays in its current status and the caller can retry or escalate.

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
                "no CWD for captain review: item has no worktree and no projects configured"
            )
        })?;

    // Pre-validate the prompt template exists before committing side effects.
    // Full render happens inside tokio::spawn (needs async evidence/knowledge data),
    // but catching a missing template here prevents stuck CaptainReviewing state.
    if !workflow.prompts.contains_key("captain_review") {
        anyhow::bail!("captain_review prompt template missing from workflow");
    }

    // Parse the trigger up-front: an unknown trigger means a newer component
    // produced a label that captain_review cannot classify, and silently
    // accepting it would cause every such review to escalate with no hint
    // why. Fail the spawn instead.
    let parsed_trigger: mando_types::task::ReviewTrigger = trigger
        .parse()
        .map_err(|e| anyhow::anyhow!("captain_review: unknown trigger {trigger:?}: {e}"))?;

    // --- Gather worker context (PR data, stream tail, process status) ---
    // This must run BEFORE any state mutation: build_single_context can fail
    // (e.g. on an unparseable worker_started_at). If we flipped the item to
    // CaptainReviewing first and the context build then errored, callers like
    // action_contract::trigger_review bubble the error up with no rollback,
    // leaving the task stuck in CaptainReviewing with no review session.
    let (_ctx, worker_contexts_text) = review_phase::build_single_context(item, config).await?;

    // All fallible operations succeeded, now commit state changes.
    // Use db_status if provided (callers that called reset_review_retry before
    // us pass the pre-reset status), otherwise use the item's current status.
    let guard_status = db_status
        .map(|s| s.to_string())
        .unwrap_or_else(|| item.status.as_str().to_string());
    let prev_status = item.status;
    let saved_last_activity = item.last_activity_at.clone();
    item.status = ItemStatus::CaptainReviewing;
    item.captain_review_trigger = Some(parsed_trigger);
    item.last_activity_at = Some(mando_types::now_rfc3339());
    let task_id = item.id.to_string();
    let task_id_num = item.id;
    let session_id = mando_uuid::Uuid::v4().to_string();
    item.session_ids.review = Some(session_id.clone());

    let event = mando_types::timeline::TimelineEvent {
        event_type: TimelineEventType::CaptainReviewStarted,
        timestamp: mando_types::now_rfc3339(),
        actor: "captain".to_string(),
        summary: format!("Captain review started (trigger: {trigger})"),
        data: serde_json::json!({ "trigger": trigger, "session_id": session_id }),
    };
    match mando_db::queries::tasks::persist_status_transition(pool, item, &guard_status, &event)
        .await
    {
        Ok(true) => {
            notifier
                .normal(&format!(
                    "\u{1f50d} Captain reviewing <b>{}</b> (trigger: {trigger})",
                    escaped_title(item),
                ))
                .await;
        }
        Ok(false) => {
            // Roll back all speculative in-memory mutations so the end-of-tick
            // write-back doesn't persist a never-spawned review session.
            item.status = prev_status;
            item.captain_review_trigger = None;
            item.session_ids.review = None;
            item.last_activity_at = saved_last_activity.clone();
            tracing::info!(
                module = "captain",
                item_id = item.id,
                "review spawn transition already applied"
            );
            return Ok(());
        }
        Err(e) => {
            item.status = prev_status;
            item.captain_review_trigger = None;
            item.session_ids.review = None;
            item.last_activity_at = saved_last_activity;
            return Err(anyhow::anyhow!(
                "persist_status_transition failed for review spawn: {e}"
            ));
        }
    }

    // Pick a credential for this review session so it goes through
    // multi-credential load balancing when credentials are configured.
    let credential = super::tick_spawn::pick_credential(pool, None).await;
    let cred_id = credential.as_ref().map(|c| c.0);

    // Log "running" session entry eagerly so (a) cancel can find it
    // immediately and (b) timeline never references a missing session.
    if let Err(e) = crate::io::headless_cc::log_running_session(
        pool,
        &session_id,
        &cwd,
        "captain-review-async",
        "",
        Some(item.id),
        false,
        cred_id,
    )
    .await
    {
        warn!(module = "captain", %session_id, %e, "failed to log running session");
    }

    // Clone data needed by the spawned task. Pre-stringify values that don't
    // depend on async I/O so they're computed once per review instead of on
    // every spawn closure run.
    let trigger_str = trigger.to_string();
    let item_title = item.title.clone();
    let item_id = item.id.to_string();

    // Build problem statement from task metadata.
    let problem_statement = {
        let mut parts = vec![item.title.clone()];
        if let Some(ref ctx) = item.context {
            parts.push(ctx.clone());
        }
        if let Some(ref prompt) = item.original_prompt {
            parts.push(prompt.clone());
        }
        parts.join("\n\n")
    };

    // Build evidence file listing from DB artifacts and detect evidence types.
    let db_evidence_listing = {
        let artifacts = mando_db::queries::artifacts::list_for_task(pool, item.id)
            .await
            .unwrap_or_default();
        let data_dir = mando_types::data_dir();
        let mut listing = String::new();
        let mut has_screenshot = false;
        let mut has_recording = false;
        use super::review_phase_artifacts::{RECORDING_EXTS, SCREENSHOT_EXTS};
        let freshness_threshold = item.reopened_at.as_deref().unwrap_or("");
        let is_reopened = item.reopen_seq > 0 && item.reopened_at.is_some();
        for artifact in &artifacts {
            if artifact.artifact_type == mando_types::ArtifactType::Evidence {
                let is_fresh = !is_reopened || artifact.created_at.as_str() > freshness_threshold;
                for media in &artifact.media {
                    let ext_lower = media.ext.to_lowercase();
                    if is_fresh && SCREENSHOT_EXTS.contains(&ext_lower.as_str()) {
                        has_screenshot = true;
                    }
                    if is_fresh && RECORDING_EXTS.contains(&ext_lower.as_str()) {
                        has_recording = true;
                    }
                    if let Some(ref local) = media.local_path {
                        let caption = media.caption.as_deref().unwrap_or("(no caption)");
                        listing.push_str(&format!(
                            "- {} ({})\n",
                            data_dir.join(local).display(),
                            caption
                        ));
                    }
                }
            }
        }
        // Latest work summary content.
        let latest_summary = artifacts
            .iter()
            .rfind(|a| a.artifact_type == mando_types::ArtifactType::WorkSummary)
            .map(|a| a.content.clone())
            .unwrap_or_default();
        (listing, latest_summary, has_screenshot, has_recording)
    };
    let (evidence_file_listing, work_summary_content, has_screenshot, has_recording) =
        db_evidence_listing;
    let intervention_count_str = item.intervention_count.to_string();
    let trigger_flags: Vec<(String, String)> = TRIGGERS
        .iter()
        .map(|name| {
            let key = format!("is_{name}");
            let flag = if trigger_str == *name {
                "true".to_string()
            } else {
                String::new()
            };
            (key, flag)
        })
        .collect();
    let timeout = workflow.agent.captain_review_timeout_s;
    let prompts = workflow.prompts.clone();
    let captain_model = workflow.models.captain.clone();
    let pool = pool.clone();
    let review_notifier = notifier.fork();

    let session_id_for_panic = session_id.clone();
    // TRACKED: detached captain-review CC session. Not registered with the
    // gateway's TaskTracker because mando-captain is a library crate and has
    // no dependency on AppState. On shutdown the external CC process is killed
    // via the pid registry; this task writes its final verdict to the stream
    // file which persists across restarts, so no in-memory state is lost.
    tokio::spawn(async move {
        let result = AssertUnwindSafe(async move {
        // Evidence is now managed by the CLI (mando todo evidence) and
        // served from the DB. The evidence_file_listing and work_summary_content
        // were pre-computed from DB before the spawn.
        let evidence_listing = evidence_file_listing.clone();

        // Load knowledge base.
        let knowledge_path = mando_config::state_dir().join("knowledge.md");
        let knowledge_base = match tokio::fs::read_to_string(&knowledge_path).await {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => {
                warn!(module = "captain", error = %e, "failed to read knowledge.md");
                String::new()
            }
        };

        // Assemble template variables. Values that don't depend on async I/O
        // were pre-computed before `tokio::spawn`; we only insert references
        // here. `FxHashMap` keyed by `&str` with owned `String` values gives
        // the hot path a faster hasher without fighting the borrow checker over
        // per-call-site lifetimes.
        let mut vars: FxHashMap<&str, String> = FxHashMap::default();
        vars.insert("trigger", trigger_str.clone());
        vars.insert("title", item_title.clone());
        vars.insert("item_id", item_id.clone());
        vars.insert("worker_contexts", worker_contexts_text.clone());
        vars.insert("knowledge_base", knowledge_base.clone());
        vars.insert("evidence_images", evidence_listing.clone());
        vars.insert("problem_statement", problem_statement.clone());
        vars.insert("evidence_files", evidence_file_listing.clone());
        vars.insert("work_summary", work_summary_content.clone());
        vars.insert("intervention_count", intervention_count_str.clone());
        vars.insert(
            "has_screenshot",
            if has_screenshot { "true" } else { "false" }.into(),
        );
        vars.insert(
            "has_recording",
            if has_recording { "true" } else { "false" }.into(),
        );
        for (key, flag) in &trigger_flags {
            vars.insert(key.as_str(), flag.clone());
        }

        let prompt = match mando_config::render_prompt("captain_review", &prompts, &vars) {
            Ok(p) => p,
            Err(e) => {
                warn!(module = "captain", %session_id, %e, "failed to render captain review prompt");
                let stream_path = mando_config::stream_path_for_session(&session_id);
                mando_cc::write_error_result(
                    &stream_path,
                    &format!("failed to render captain review prompt: {e}"),
                );
                return;
            }
        };

        let builder = mando_cc::CcConfig::builder()
            .model(&captain_model)
            .timeout(timeout)
            .caller("captain-review-async")
            .task_id(&task_id)
            .cwd(cwd.clone())
            .session_id(session_id.clone())
            .allowed_tools(vec!["Read".into(), "Bash".into()])
            .disallowed_tools(vec!["Agent".into()])
            .json_schema(verdict_json_schema(&trigger_str));
        let config = super::tick_spawn::with_credential(builder, &credential).build();

        let sid_for_hook = session_id.clone();
        match mando_cc::CcOneShot::run_with_pid_hook(&prompt, config, |pid| {
            if let Err(e) = crate::io::pid_registry::register(&sid_for_hook, pid) {
                warn!(module = "captain", sid = %sid_for_hook, %e, "pid_registry register failed");
            }
        })
        .await
        {
            Ok(result) => {
                info!(module = "captain", %session_id, "captain review CC completed");
                if let Err(e) = crate::io::pid_registry::unregister(&session_id) {
                    warn!(module = "captain", %session_id, %e, "pid_registry unregister failed");
                }
                let cred_id = mando_db::queries::sessions::get_credential_id(&pool, &session_id)
                    .await
                    .unwrap_or(None);
                review_notifier
                    .check_rate_limit(&result, &pool, cred_id)
                    .await;
                if let Err(e) = crate::io::headless_cc::log_cc_result(
                    &pool,
                    &result,
                    &cwd,
                    "captain-review-async",
                    Some(task_id_num),
                )
                .await {
                    warn!(module = "captain", %session_id, %e, "log_cc_result failed");
                }
            }
            Err(e) => {
                warn!(module = "captain", %session_id, %e, "captain review CC failed");
                if let Err(e2) = crate::io::pid_registry::unregister(&session_id) {
                    warn!(module = "captain", %session_id, %e2, "pid_registry unregister failed");
                }
                // Write a synthetic error result so check_review() finds it on
                // the next tick instead of waiting for the full timeout.
                let stream_path = mando_config::stream_path_for_session(&session_id);
                mando_cc::write_error_result(
                    &stream_path,
                    &format!("captain review CC process failed: {e}"),
                );
                if let Err(e2) = crate::io::headless_cc::log_cc_failure(
                    &pool,
                    &session_id,
                    &cwd,
                    "captain-review-async",
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
                "captain review spawn panicked: {:?}",
                panic
            );
            let stream_path = mando_config::stream_path_for_session(&session_id_for_panic);
            mando_cc::write_error_result(
                &stream_path,
                &format!("captain review spawn panicked: {:?}", panic),
            );
        }
    });

    Ok(())
}

#[cfg(test)]
#[path = "captain_review_tests.rs"]
mod tests;
