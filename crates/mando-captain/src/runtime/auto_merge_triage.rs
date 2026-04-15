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
use mando_db::queries::timeline::AppendResult;
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
            "apply_triage_verdict called with no triage session id — skipping"
        );
        return;
    };
    let is_high = result.confidence == "high";

    let dedupe_key =
        mando_db::queries::timeline::auto_merge_triage_dedupe_key(item.id, &session_id);
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
    // `AlreadyExists` happens after a daemon restart between event-write and
    // session_id clear: the verdict is already recorded, so we proceed to
    // clear the session and run the post-write side effects (idempotent).
    // A real `Err` means the row didn't land — leave session_id set so the
    // next tick re-reads the (still-successful) CC stream and retries.
    match mando_db::queries::timeline::append_with_dedupe_key(pool, item.id, &event, &dedupe_key)
        .await
    {
        Ok(AppendResult::Inserted | AppendResult::AlreadyExists) => {}
        Err(e) => {
            warn!(
                module = "captain",
                item_id = item.id,
                error = %e,
                "failed to persist auto_merge_triage verdict; will retry next tick"
            );
            return;
        }
    }

    // Clear the triage session so the poller won't re-process it.
    item.session_ids.triage = None;

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
        // Caller (poll_triage) checked this is Some, so a None here is a bug.
        // Bail loudly rather than write an empty session_id into the timeline.
        warn!(
            module = "captain",
            item_id = item.id,
            "apply_triage_failure called with no triage session id — skipping"
        );
        return;
    };

    // Compute the 1-indexed attempt number from the timeline state. If this
    // load fails we cannot determine the attempt number safely (defaulting to
    // 1 would create a duplicate dedupe key for an already-recorded attempt 1
    // and silently drop the failure). Skip persistence; the next tick retries.
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

    let dedupe_key =
        mando_db::queries::timeline::auto_merge_triage_failed_dedupe_key(item.id, &session_id);
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
    let append = match mando_db::queries::timeline::append_with_dedupe_key(
        pool,
        item.id,
        &event,
        &dedupe_key,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            // Genuine DB error (not a UNIQUE conflict): leave the triage
            // session id in place so the next tick retries reading the
            // (still-erroring) CC stream and re-attempts the persist.
            // Skipping the clear is what protects the retry budget.
            warn!(
                module = "captain",
                item_id = item.id,
                error = %e,
                "failed to persist auto_merge_triage_failed event; will retry next tick"
            );
            return;
        }
    };

    // Clear the triage session so the next tick can re-spawn after backoff.
    // Idempotent: if `AlreadyExists`, a prior tick already wrote the failure
    // and ran the side effects — clear the session and skip the duplicate
    // notification so we don't spam Telegram on every restart.
    item.session_ids.triage = None;

    if append == AppendResult::AlreadyExists {
        info!(
            module = "captain",
            item_id = item.id,
            attempt,
            "auto_merge_triage_failed already recorded; clearing session without re-notifying"
        );
        return;
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
/// `last_failure_at` is the timestamp of the most recent
/// `AutoMergeTriageFailed` event in the current cycle. It's part of the
/// dedupe key so two exhaustions in cycles separated by a `ReworkRequested`
/// (which opens a fresh cycle without bumping `reopen_seq`) don't collide
/// on the UNIQUE index.
pub(crate) async fn emit_exhaustion(
    item: &Task,
    last_error: Option<&str>,
    last_failure_at: &str,
    attempts: u32,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) {
    let dedupe_key = mando_db::queries::timeline::auto_merge_triage_exhausted_dedupe_key(
        item.id,
        item.reopen_seq,
        last_failure_at,
    );
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
    let append = match mando_db::queries::timeline::append_with_dedupe_key(
        pool,
        item.id,
        &event,
        &dedupe_key,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            // DB write failed: don't fire the high-priority notification.
            // The next tick's `decide_spawn` will return `EmitExhausted`
            // again (since no exhaustion event was recorded), giving us a
            // natural retry. Without this early return, every subsequent
            // tick would spam the human while the DB is unhealthy.
            warn!(
                module = "captain",
                item_id = item.id,
                error = %e,
                "failed to persist auto_merge_triage_exhausted event; will retry next tick"
            );
            return;
        }
    };

    // Idempotent: if we crashed after writing the event but before the
    // mergeability tick recorded it in derived state, the next tick re-fires
    // `EmitExhausted`. The row already exists, so we skip the duplicate
    // Telegram alert.
    if append == AppendResult::AlreadyExists {
        info!(
            module = "captain",
            item_id = item.id,
            attempts,
            "auto_merge_triage_exhausted already recorded; skipping duplicate notification"
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
