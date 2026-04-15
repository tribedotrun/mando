//! Auto-merge triage -- spawn a short-lived CC session to evaluate whether
//! an AwaitingReview task can be merged without human review.
//!
//! Lifecycle:
//! - A "cycle" opens when the task first enters AwaitingReview and after
//!   every human-initiated reopen/rework. Auto-reopens (`review`, `ci`,
//!   `evidence`) do NOT open a new cycle.
//! - Within a cycle, the poller re-spawns on CC error / timeout / malformed
//!   output up to `auto_merge_triage_max_attempts` with `auto_merge_triage_backoff_s`
//!   waits between attempts. A successful verdict closes the cycle.
//! - On exhaustion: emit `AutoMergeTriageExhausted`, notify the human, stay
//!   in AwaitingReview. No synthetic low-confidence verdict is ever written.
//!
//! Pure gate logic (state derivation + spawn decision) lives in the sibling
//! `auto_merge_triage_gate` module so it can be unit-tested without tokio,
//! DB, or CC dependencies.

use tracing::{info, warn};

use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::task::{ItemStatus, Task};
use mando_types::timeline::{TimelineEvent, TimelineEventType};

use super::auto_merge_triage_gate::{extract_cc_error_text, TriageOutcome, TriageResult};
use super::notify::Notifier;

pub(crate) use super::auto_merge_triage_gate::{
    decide_spawn, derive_gate_state, last_failure_error, triage_json_schema, SpawnDecision,
};
pub(crate) use super::auto_merge_triage_spawn::spawn_triage;

/// Inspect the CC stream result for a triage session and classify it.
/// Returns `None` if the session hasn't finished yet.
fn check_triage_outcome(item: &Task) -> Option<TriageOutcome> {
    let session_id = item.session_ids.triage.as_deref()?;
    let stream_path = mando_config::stream_path_for_session(session_id);
    let result = mando_cc::get_stream_result(&stream_path)?;

    // Failure path: CC errored (stream timeout, crash, spawn panic, etc.)
    if result.get("is_error").and_then(|v| v.as_bool()) == Some(true) {
        let err_text = extract_cc_error_text(&result);
        return Some(TriageOutcome::Failed(err_text));
    }

    // Success path: prefer structured_output, then fall back to parsing the
    // text `result` field.
    if let Some(so) = result.get("structured_output").filter(|v| !v.is_null()) {
        match serde_json::from_value::<TriageResult>(so.clone()) {
            Ok(tr) => return Some(TriageOutcome::Verdict(tr)),
            Err(e) => {
                warn!(module = "captain", %e, %session_id, "triage structured_output parse failed; falling back to text");
            }
        }
    }

    let mut text = result
        .get("result")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if text.is_empty() {
        if let Some(t) = mando_cc::get_last_assistant_text(&stream_path) {
            text = t;
        } else {
            return Some(TriageOutcome::Failed(
                "Triage completed with no output".to_string(),
            ));
        }
    }
    match serde_json::from_str::<TriageResult>(&text) {
        Ok(tr) => Some(TriageOutcome::Verdict(tr)),
        Err(e) => Some(TriageOutcome::Failed(format!(
            "Malformed triage output: {e}"
        ))),
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

        // 1. Did CC finish (with or without error)?
        if let Some(outcome) = check_triage_outcome(item) {
            match outcome {
                TriageOutcome::Verdict(verdict) => {
                    apply_triage_verdict(item, &verdict, config, notifier, pool).await;
                }
                TriageOutcome::Failed(err) => {
                    apply_triage_failure(item, &err, notifier, pool).await;
                }
            }
            continue;
        }

        // 2. Triage-side idle timeout (CC stream went quiet past the budget).
        // If the timestamp is missing or unparseable, treat as timed out: we
        // can't trust the freshness signal, and leaving the session pending
        // forever would orphan auto-merge for this cycle.
        let is_timed_out = match item.last_activity_at.as_deref() {
            Some(ts) => match time::OffsetDateTime::parse(
                ts,
                &time::format_description::well_known::Rfc3339,
            ) {
                Ok(entered) => {
                    let elapsed = time::OffsetDateTime::now_utc() - entered;
                    elapsed.whole_seconds() as u64 > triage_timeout.as_secs()
                }
                Err(e) => {
                    warn!(
                        module = "captain",
                        item_id = item.id,
                        last_activity_at = %ts,
                        error = %e,
                        "triage last_activity_at unparseable; treating as timed out"
                    );
                    true
                }
            },
            None => {
                warn!(
                    module = "captain",
                    item_id = item.id,
                    "triage last_activity_at missing; treating as timed out"
                );
                true
            }
        };

        if is_timed_out {
            warn!(
                module = "captain",
                item_id = item.id,
                "auto-merge triage session timed out"
            );
            apply_triage_failure(
                item,
                &format!(
                    "Triage session timed out after {}s with no response",
                    triage_timeout.as_secs()
                ),
                notifier,
                pool,
            )
            .await;
        }
    }
}

/// Apply a successful verdict — emit timeline event, transition to
/// CaptainMerging if confidence is high.
async fn apply_triage_verdict(
    item: &mut Task,
    result: &TriageResult,
    config: &Config,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) {
    let Some(session_id) = item.session_ids.triage.clone() else {
        warn!(
            module = "captain",
            item_id = item.id,
            "apply_triage_verdict called with no triage session id -- skipping"
        );
        return;
    };
    let is_high = result.confidence == "high";

    let event = TimelineEvent {
        event_type: TimelineEventType::AutoMergeTriage,
        timestamp: mando_types::now_rfc3339(),
        actor: "captain".to_string(),
        summary: format!("Auto-merge triage: {} confidence", result.confidence),
        data: serde_json::json!({
            "confidence": result.confidence,
            "reason": result.reason,
            "session_id": session_id,
            "reopen_seq": item.reopen_seq,
        }),
    };
    // Atomically write the event AND clear session_ids.triage in one tx.
    // No crash window: either both happen or neither does. On crash-restart
    // the triage session is still set, the CC stream is still on disk, and
    // the next tick re-processes from scratch.
    item.session_ids.triage = None;
    let session_ids_json = item.session_ids.to_json();
    match mando_db::queries::timeline::append_and_clear_triage_session(
        pool,
        item.id,
        &event,
        &session_ids_json,
    )
    .await
    {
        Ok(_) => {}
        Err(e) => {
            // Restore session so next tick retries.
            item.session_ids.triage = Some(session_id);
            warn!(
                module = "captain",
                item_id = item.id,
                error = %e,
                "failed to persist auto_merge_triage verdict; will retry next tick"
            );
            return;
        }
    }

    if is_high && config.captain.auto_merge && !item.no_auto_merge {
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

/// Apply a failed attempt — emit `AutoMergeTriageFailed`, clear the session,
/// notify normal priority. Does NOT synthesize a verdict. The next spawn tick
/// will either re-spawn after backoff or emit exhaustion.
///
/// Important: if the timeline read or write fails, we deliberately leave
/// `session_ids.triage` set so the next tick polls the same CC stream again
/// and re-attempts persistence. Clearing on a write failure would let the
/// retry budget be silently bypassed (the next spawn would think failure
/// count is one lower than it actually is).
async fn apply_triage_failure(
    item: &mut Task,
    err_text: &str,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) {
    let Some(session_id) = item.session_ids.triage.clone() else {
        warn!(
            module = "captain",
            item_id = item.id,
            "apply_triage_failure called with no triage session id -- skipping"
        );
        return;
    };

    // Compute the 1-indexed attempt number from the timeline state.
    let attempt = match mando_db::queries::timeline::load_triage_gate_events(pool, item.id).await {
        Ok(events) => derive_gate_state(&events).failures_in_cycle + 1,
        Err(e) => {
            warn!(
                module = "captain",
                item_id = item.id,
                error = %e,
                "failed to load triage events for attempt counting; will retry next tick"
            );
            return;
        }
    };

    let event = TimelineEvent {
        event_type: TimelineEventType::AutoMergeTriageFailed,
        timestamp: mando_types::now_rfc3339(),
        actor: "captain".to_string(),
        summary: format!("Auto-merge triage attempt {attempt} failed"),
        data: serde_json::json!({
            "error": err_text,
            "attempt": attempt,
            "session_id": session_id,
            "reopen_seq": item.reopen_seq,
        }),
    };
    // Atomically write the failure event AND clear session_ids.triage.
    item.session_ids.triage = None;
    let session_ids_json = item.session_ids.to_json();
    match mando_db::queries::timeline::append_and_clear_triage_session(
        pool,
        item.id,
        &event,
        &session_ids_json,
    )
    .await
    {
        Ok(_) => {}
        Err(e) => {
            item.session_ids.triage = Some(session_id);
            warn!(
                module = "captain",
                item_id = item.id,
                error = %e,
                "failed to persist auto_merge_triage_failed event; will retry next tick"
            );
            return;
        }
    }

    warn!(
        module = "captain",
        item_id = item.id,
        attempt,
        error = %err_text,
        "auto-merge triage attempt failed; will retry after backoff"
    );

    let title = mando_shared::telegram_format::escape_html(&item.title);
    let err_escaped = mando_shared::telegram_format::escape_html(err_text);
    notifier
        .normal(&format!(
            "\u{26a0}\u{fe0f} Auto-merge triage attempt {attempt} failed for <b>{title}</b>: {err_escaped}"
        ))
        .await;
}

/// Emit an `AutoMergeTriageExhausted` event and notify the human.
/// Called from the spawn path when attempts are at the cap.
///
/// Idempotency is handled by `derive_gate_state`: once the exhaustion event
/// exists in the timeline, `decide_spawn` returns `Skip`, preventing re-emission.
pub(crate) async fn emit_exhaustion(
    item: &Task,
    last_error: Option<&str>,
    attempts: u32,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) {
    let event = TimelineEvent {
        event_type: TimelineEventType::AutoMergeTriageExhausted,
        timestamp: mando_types::now_rfc3339(),
        actor: "captain".to_string(),
        summary: format!("Auto-merge triage exhausted after {attempts} attempts"),
        data: serde_json::json!({
            "attempts": attempts,
            "last_error": last_error.unwrap_or(""),
            "reopen_seq": item.reopen_seq,
        }),
    };
    if let Err(e) = mando_db::queries::timeline::append(pool, item.id, &event).await {
        warn!(
            module = "captain",
            item_id = item.id,
            error = %e,
            "failed to persist auto_merge_triage_exhausted event; will retry next tick"
        );
        return;
    }

    let title = mando_shared::telegram_format::escape_html(&item.title);
    let err_clause = match last_error {
        Some(e) if !e.is_empty() => format!(": {}", mando_shared::telegram_format::escape_html(e)),
        _ => String::new(),
    };
    notifier
        .high(&format!(
            "\u{1f6d1} Auto-merge triage exhausted after {attempts} attempts for <b>{title}</b>{err_clause} — human review needed"
        ))
        .await;

    info!(
        module = "captain",
        item_id = item.id,
        attempts,
        "auto-merge triage exhausted; human review needed"
    );
}

pub(super) use super::auto_merge_triage_merge::transition_to_merging;
