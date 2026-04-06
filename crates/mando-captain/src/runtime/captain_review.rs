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
use super::timeline_emit;
use crate::io::evidence;

use super::captain_review_verdict::escaped_title;
pub use super::captain_review_verdict::{apply_verdict, handle_review_error};

/// Structured verdict from a captain review CC session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptainVerdict {
    pub action: String,
    pub feedback: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<String>,
}

fn is_verdict_allowed(trigger: &str, action: &str) -> bool {
    // Captain is the last line of defense — it must solve problems, not punt
    // them. Escalate and retry_clarifier are restricted to specific triggers.
    match trigger {
        "clarifier_fail" => matches!(action, "retry_clarifier" | "escalate"),
        "budget_exhausted" => matches!(
            action,
            "ship" | "nudge" | "respawn" | "reset_budget" | "escalate"
        ),
        // All other triggers: captain must act. No escalation, no retry_clarifier.
        _ => matches!(action, "ship" | "nudge" | "respawn" | "reset_budget"),
    }
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
pub(crate) async fn spawn_review(
    item: &mut Task,
    trigger: &str,
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
    let (ctx, worker_contexts_text) = review_phase::build_single_context(item, config).await?;

    // All fallible operations succeeded, now commit state changes.
    item.status = ItemStatus::CaptainReviewing;
    item.captain_review_trigger = Some(parsed_trigger);
    item.last_activity_at = Some(mando_types::now_rfc3339());
    let pr_body_for_evidence = ctx.pr_body.clone();
    let worker_name_owned = item.worker.as_deref().unwrap_or("unknown").to_string();

    let task_id = item.id.to_string();
    let session_id = mando_uuid::Uuid::v4().to_string();
    item.session_ids.review = Some(session_id.clone());

    let _ = timeline_emit::emit_for_task(
        item,
        TimelineEventType::CaptainReviewStarted,
        &format!("Captain review started (trigger: {trigger})"),
        serde_json::json!({ "trigger": trigger, "session_id": session_id }),
        pool,
    )
    .await;
    notifier
        .normal(&format!(
            "\u{1f50d} Captain reviewing <b>{}</b> (trigger: {trigger})",
            escaped_title(item),
        ))
        .await;

    // Log "running" session entry eagerly so (a) cancel can find it
    // immediately and (b) timeline never references a missing session.
    if let Err(e) = crate::io::headless_cc::log_running_session(
        pool,
        &session_id,
        &cwd,
        "captain-review-async",
        "",
        &item.id.to_string(),
        false,
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
    let evidence_dl_timeout = workflow.agent.evidence_download_timeout_s;
    let evidence_ff_timeout = workflow.agent.evidence_ffmpeg_timeout_s;
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
        // Download evidence images from PR body.
        let work_dir = mando_config::state_dir().join("captain-evidence");
        let worker_dir = work_dir.join(&worker_name_owned);
        let evidence_download = evidence::download_evidence(
            &pr_body_for_evidence,
            &worker_dir,
            evidence_dl_timeout,
            evidence_ff_timeout,
        )
        .await;
        let evidence_paths = evidence_download.paths;
        let evidence_listing = if evidence_paths.is_empty() {
            String::new()
        } else {
            let mut listing = format!("\n**{}**:\n", worker_name_owned);
            for path in &evidence_paths {
                listing.push_str(&format!("- {}\n", path.display()));
            }
            listing
        };

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
        vars.insert("intervention_count", intervention_count_str.clone());
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

        let config = mando_cc::CcConfig::builder()
            .model(&captain_model)
            .timeout(timeout)
            .caller("captain-review-async")
            .task_id(&task_id)
            .cwd(cwd.clone())
            .session_id(session_id.clone())
            .allowed_tools(vec!["Read".into()])
            .json_schema(verdict_json_schema(&trigger_str))
            .build();

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
                review_notifier.check_rate_limit(&result).await;
                if let Err(e) = crate::io::headless_cc::log_cc_result(
                    &pool,
                    &result,
                    &cwd,
                    "captain-review-async",
                    &task_id,
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
                    &task_id,
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

/// Check if a captain review has completed. Returns the verdict if done.
pub(crate) fn check_review(item: &Task) -> Option<CaptainVerdict> {
    let session_id = item.session_ids.review.as_deref()?;
    let stream_path = mando_config::stream_path_for_session(session_id);
    let result = mando_cc::get_stream_result(&stream_path)?;

    // Skip error results — handled separately by check_review_failed().
    if result.get("is_error").and_then(|v| v.as_bool()) == Some(true) {
        return None;
    }

    // Try structured_output first (populated when --json-schema was used).
    if let Some(so) = result.get("structured_output").filter(|v| !v.is_null()) {
        match serde_json::from_value::<CaptainVerdict>(so.clone()) {
            Ok(verdict) => return Some(validate_verdict(verdict, item)),
            Err(e) => {
                let raw_preview: String = so.to_string().chars().take(300).collect();
                warn!(module = "captain", %e, %session_id, raw = %raw_preview,
                    "structured_output present but failed to parse, trying fallbacks");
            }
        }
    }

    // Fall back to result text field.
    let mut verdict_text = result
        .get("result")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // If result field is empty, recover from the last assistant text block.
    if verdict_text.is_empty() {
        if let Some(text) = mando_cc::get_last_assistant_text(&stream_path) {
            warn!(module = "captain", %session_id,
                "check_review: result field empty, recovered from last assistant text");
            verdict_text = text;
        } else {
            // Session completed but produced no extractable verdict — escalate.
            warn!(module = "captain", %session_id,
                "check_review: session completed but all extraction paths empty, escalating");
            return Some(CaptainVerdict {
                action: "escalate".into(),
                feedback: "Captain review session completed but produced no extractable verdict"
                    .into(),
                report: Some(
                    "Captain review session completed but all extraction paths \
                     (structured_output, result text, last assistant text) were empty. \
                     The CC session may have failed silently or produced no usable output. \
                     Manual review required."
                        .into(),
                ),
            });
        }
    }

    match serde_json::from_str::<CaptainVerdict>(&verdict_text) {
        Ok(verdict) => Some(validate_verdict(verdict, item)),
        Err(e) => {
            warn!(module = "captain", %e,
                preview = &verdict_text[..verdict_text.floor_char_boundary(200)],
                "failed to parse captain review verdict, defaulting to escalate");
            Some(CaptainVerdict {
                action: "escalate".into(),
                feedback: format!("Failed to parse review verdict: {e}"),
                report: Some(format!(
                    "Captain review verdict could not be parsed as JSON. \
                     Raw text (first 200 chars): {}",
                    &verdict_text[..verdict_text.floor_char_boundary(200)]
                )),
            })
        }
    }
}

/// Check if the async CC task wrote an error result to the stream file.
///
/// Returns the error message if a failure marker is present.
pub(crate) fn check_review_failed(item: &Task) -> Option<String> {
    let session_id = item.session_ids.review.as_deref()?;
    let stream_path = mando_config::stream_path_for_session(session_id);
    let result = mando_cc::get_stream_result(&stream_path)?;
    if result.get("is_error").and_then(|v| v.as_bool()) != Some(true) {
        return None;
    }
    let msg = result
        .get("error")
        .and_then(|v| v.as_str())
        .unwrap_or("CC process failed")
        .to_string();
    warn!(module = "captain", %session_id, %msg, "captain review async task failed");
    Some(msg)
}

/// Validate a parsed verdict against the trigger's allowed actions.
fn validate_verdict(verdict: CaptainVerdict, item: &Task) -> CaptainVerdict {
    let trigger = item
        .captain_review_trigger
        .map(|t| t.as_str())
        .unwrap_or("unknown");
    if !is_verdict_allowed(trigger, &verdict.action) {
        warn!(module = "captain", action = %verdict.action, %trigger,
            "verdict not allowed for trigger, defaulting to escalate");
        CaptainVerdict {
            action: "escalate".into(),
            feedback: format!(
                "Invalid action '{}' for trigger '{trigger}'. {}",
                verdict.action, verdict.feedback
            ),
            report: Some(verdict.report.unwrap_or_else(|| {
                format!(
                    "Captain review returned invalid action '{}' for trigger '{trigger}'. \
                     Original feedback: {}",
                    verdict.action, verdict.feedback
                )
            })),
        }
    } else {
        verdict
    }
}

#[cfg(test)]
#[path = "captain_review_tests.rs"]
mod tests;
