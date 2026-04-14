//! Auto-merge triage -- spawn a short-lived CC session to evaluate whether
//! an AwaitingReview task can be merged without human review.
//!
//! Spawn: when a PR is mergeable and `config.captain.auto_merge` is on.
//! Poll:  on subsequent ticks, check if the triage session produced a result.
//! High confidence triggers CaptainMerging; otherwise task stays in AwaitingReview.

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::task::{ItemStatus, Task};
use mando_types::timeline::TimelineEventType;

use super::notify::Notifier;

pub(crate) use super::auto_merge_triage_spawn::spawn_triage;

/// Structured output from the triage agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TriageResult {
    pub confidence: String,
    pub reason: String,
}

/// JSON Schema for the triage structured output.
pub(super) fn triage_json_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "confidence": {
                "type": "string",
                "enum": ["high", "mid", "low"],
                "description": "Confidence that this task can be merged without human review"
            },
            "reason": {
                "type": "string",
                "description": "Brief explanation of the confidence assessment"
            }
        },
        "required": ["confidence", "reason"]
    })
}

/// Check if a triage session has completed. Returns the result if done.
fn check_triage(item: &Task) -> Option<TriageResult> {
    let session_id = item.session_ids.triage.as_deref()?;
    let stream_path = mando_config::stream_path_for_session(session_id);
    let result = mando_cc::get_stream_result(&stream_path)?;

    // Skip error results -- handled separately by check_triage_failed().
    if result.get("is_error").and_then(|v| v.as_bool()) == Some(true) {
        return None;
    }

    // Try structured_output first.
    if let Some(so) = result.get("structured_output").filter(|v| !v.is_null()) {
        match serde_json::from_value::<TriageResult>(so.clone()) {
            Ok(tr) => return Some(tr),
            Err(e) => {
                warn!(module = "captain", %e, %session_id, "triage structured_output parse failed");
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
        if let Some(t) = mando_cc::get_last_assistant_text(&stream_path) {
            text = t;
        } else {
            return Some(TriageResult {
                confidence: "low".into(),
                reason: "Triage session completed but produced no output".into(),
            });
        }
    }

    match serde_json::from_str::<TriageResult>(&text) {
        Ok(tr) => Some(tr),
        Err(e) => {
            warn!(module = "captain", %e, "failed to parse triage result");
            Some(TriageResult {
                confidence: "low".into(),
                reason: format!("Failed to parse triage result: {e}"),
            })
        }
    }
}

/// Check if a triage session has failed (stream file has error result).
fn check_triage_failed(item: &Task) -> Option<String> {
    let session_id = item.session_ids.triage.as_deref()?;
    let stream_path = mando_config::stream_path_for_session(session_id);
    let result = mando_cc::get_stream_result(&stream_path)?;
    let is_error = result
        .get("is_error")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if is_error {
        let msg = result
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error")
            .to_string();
        Some(msg)
    } else {
        None
    }
}

/// Poll running triage sessions. Called at the start of mergeability check.
pub(crate) async fn poll_triage(
    items: &mut [Task],
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) {
    // Clear stale triage sessions on items that left AwaitingReview
    // (e.g., CI failure routed to captain review while triage was in-flight).
    for item in items
        .iter_mut()
        .filter(|it| it.status != ItemStatus::AwaitingReview && it.session_ids.triage.is_some())
    {
        tracing::debug!(
            module = "captain",
            item_id = item.id,
            status = %item.status.as_str(),
            "clearing stale triage session on non-AwaitingReview item"
        );
        item.session_ids.triage = None;
    }

    let triage_timeout = workflow.agent.captain_review_timeout_s;

    for item in items
        .iter_mut()
        .filter(|it| it.status == ItemStatus::AwaitingReview)
    {
        let has_session = item
            .session_ids
            .triage
            .as_deref()
            .is_some_and(|s| !s.is_empty());
        if !has_session {
            continue;
        }

        // Check for CC crash/failure -- emit a low-confidence result so
        // has_auto_merge_triage returns true and blocks re-spawning.
        if let Some(error_msg) = check_triage_failed(item) {
            warn!(
                module = "captain",
                item_id = item.id,
                error = %error_msg,
                "auto-merge triage session failed"
            );
            let failed = TriageResult {
                confidence: "low".into(),
                reason: format!("Triage session failed: {error_msg}"),
            };
            apply_triage_result(item, &failed, config, notifier, pool).await;
            continue;
        }

        // Check for completed result.
        if let Some(result) = check_triage(item) {
            apply_triage_result(item, &result, config, notifier, pool).await;
            continue;
        }

        // Check timeout.
        let is_timed_out = match item.last_activity_at.as_deref() {
            Some(ts) => match time::OffsetDateTime::parse(
                ts,
                &time::format_description::well_known::Rfc3339,
            ) {
                Ok(entered) => {
                    let elapsed = time::OffsetDateTime::now_utc() - entered;
                    elapsed.whole_seconds() as u64 > triage_timeout.as_secs()
                }
                Err(_) => false,
            },
            None => false,
        };

        if is_timed_out {
            warn!(
                module = "captain",
                item_id = item.id,
                "auto-merge triage session timed out"
            );
            let timeout = TriageResult {
                confidence: "low".into(),
                reason: "Triage session timed out".into(),
            };
            apply_triage_result(item, &timeout, config, notifier, pool).await;
        }
    }
}

/// Apply the triage result -- emit timeline event, transition to CaptainMerging
/// if confidence is high.
async fn apply_triage_result(
    item: &mut Task,
    result: &TriageResult,
    config: &Config,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) {
    let session_id = item.session_ids.triage.clone().unwrap_or_default();
    let is_high = result.confidence == "high";

    // Emit triage timeline event with a reopen_seq-keyed dedupe key so
    // `has_auto_merge_triage` can do an O(1) indexed lookup.
    let timestamp = mando_types::now_rfc3339();
    let dedupe_key = mando_db::queries::timeline::auto_merge_triage_dedupe_key(
        item.id,
        item.reopen_seq,
        &timestamp,
    );
    let event = mando_types::timeline::TimelineEvent {
        event_type: TimelineEventType::AutoMergeTriage,
        timestamp,
        actor: "captain".to_string(),
        summary: format!("Auto-merge triage: {} confidence", result.confidence),
        data: serde_json::json!({
            "confidence": result.confidence,
            "reason": result.reason,
            "session_id": session_id,
            "reopen_seq": item.reopen_seq,
        }),
    };
    if let Err(e) =
        mando_db::queries::timeline::append_with_dedupe_key(pool, item.id, &event, &dedupe_key)
            .await
    {
        warn!(
            module = "captain",
            item_id = item.id,
            error = %e,
            "failed to persist auto_merge_triage timeline event"
        );
    }

    // Clear triage session before branching so poll_triage won't re-process
    // this session regardless of whether the merge transition succeeds.
    item.session_ids.triage = None;

    if is_high && config.captain.auto_merge {
        transition_to_merging(item, result, config, notifier, pool).await;
    } else {
        info!(
            module = "captain",
            item_id = item.id,
            confidence = %result.confidence,
            auto_merge = config.captain.auto_merge,
            "auto-merge triage: not merging, leaving in AwaitingReview"
        );
    }
}

/// Transition to CaptainMerging after a high-confidence triage result.
async fn transition_to_merging(
    item: &mut Task,
    result: &TriageResult,
    config: &Config,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) {
    let pr_num = item.pr_number.unwrap_or(0);
    let repo = item
        .github_repo
        .clone()
        .or_else(|| mando_config::resolve_github_repo(Some(&item.project), config))
        .unwrap_or_default();
    let pr_url = format!("https://github.com/{repo}/pull/{pr_num}");

    let prev_status = item.status;
    item.status = ItemStatus::CaptainMerging;
    item.session_ids.merge = None;
    item.merge_fail_count = 0;
    item.last_activity_at = Some(mando_types::now_rfc3339());

    let event = mando_types::timeline::TimelineEvent {
        event_type: TimelineEventType::CaptainMergeStarted,
        timestamp: mando_types::now_rfc3339(),
        actor: "captain".to_string(),
        summary: "Auto-merge triage passed -- starting merge".to_string(),
        data: serde_json::json!({
            "pr": &pr_url,
            "source": "auto_merge_triage",
        }),
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
            let title = mando_shared::telegram_format::escape_html(&item.title);
            notifier
                .normal(&format!(
                    "\u{2705} Auto-merge triage passed for <b>{title}</b> -- merging"
                ))
                .await;
            info!(
                module = "captain",
                item_id = item.id,
                confidence = %result.confidence,
                "auto-merge triage passed, transitioning to CaptainMerging"
            );
        }
        Ok(false) => {
            item.status = prev_status;
            info!(
                module = "captain",
                item_id = item.id,
                "auto-merge triage transition already applied"
            );
        }
        Err(e) => {
            item.status = prev_status;
            warn!(
                module = "captain",
                item_id = item.id,
                error = %e,
                "failed to persist auto-merge triage transition"
            );
        }
    }
}
