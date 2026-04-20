//! Polling and validation for captain review sessions.

use tracing::warn;

use crate::Task;

use super::captain_review::CaptainVerdict;

fn is_verdict_allowed(trigger: &str, action: &str) -> bool {
    // Captain is the last line of defense -- it must solve problems, not punt
    // them. Escalate and retry_clarifier are restricted to specific triggers.
    match trigger {
        "clarifier_fail" => matches!(action, "retry_clarifier" | "escalate"),
        "spawn_fail" => matches!(action, "respawn" | "escalate"),
        "budget_exhausted" => matches!(
            action,
            "ship" | "nudge" | "respawn" | "reset_budget" | "escalate"
        ),
        // All other triggers: captain must act. No escalation, no retry_clarifier.
        _ => matches!(action, "ship" | "nudge" | "respawn" | "reset_budget"),
    }
}

/// Check if a captain review has completed. Returns the verdict if done.
pub(crate) fn check_review(item: &Task) -> Option<CaptainVerdict> {
    let session_id = item.session_ids.review.as_deref()?;
    let stream_path = global_infra::paths::stream_path_for_session(session_id);
    let result = global_claude::get_stream_result(&stream_path)?;

    // Skip error results -- handled separately by check_review_failed().
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
        if let Some(text) = global_claude::get_last_assistant_text(&stream_path) {
            warn!(module = "captain", %session_id,
                "check_review: result field empty, recovered from last assistant text");
            verdict_text = text;
        } else {
            // Session completed but produced no extractable verdict -- escalate.
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
                confidence: None,
                confidence_reason: None,
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
                confidence: None,
                confidence_reason: None,
            })
        }
    }
}

/// Check if the async CC task wrote an error result to the stream file.
///
/// Returns the error message if a failure marker is present.
pub(crate) fn check_review_failed(item: &Task) -> Option<String> {
    let session_id = item.session_ids.review.as_deref()?;
    let stream_path = global_infra::paths::stream_path_for_session(session_id);
    let result = global_claude::get_stream_result(&stream_path)?;
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
///
/// Also normalizes confidence fields on `ship`:
/// - If `confidence` is missing or not one of high/mid/low, default to "mid"
///   so the verdict still ships to AwaitingReview but does not auto-merge.
///   A missing confidence means the model forgot the rubric; we should not
///   auto-merge in that case, but we also should not block shipping and burn
///   a nudge cycle. Log a warning so the miss is visible.
/// - If `confidence_reason` is missing, synthesize a placeholder that makes
///   the miss obvious in the timeline.
pub(crate) fn validate_verdict(verdict: CaptainVerdict, item: &Task) -> CaptainVerdict {
    let trigger = item
        .captain_review_trigger
        .map(|t| t.as_str())
        .unwrap_or("unknown");
    if !is_verdict_allowed(trigger, &verdict.action) {
        warn!(module = "captain", action = %verdict.action, %trigger,
            "verdict not allowed for trigger, defaulting to escalate");
        return CaptainVerdict {
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
            confidence: None,
            confidence_reason: None,
        };
    }

    if verdict.action == "ship" {
        let mut out = verdict;
        let confidence_valid = matches!(
            out.confidence.as_deref(),
            Some("high") | Some("mid") | Some("low")
        );
        if !confidence_valid {
            // Under `budget_exhausted`, the rubric reserves `low` for forced
            // ships on weak evidence — align the default with that intent so
            // the timeline label reads honestly. Everywhere else, default to
            // `mid` (ships to AwaitingReview, skips auto-merge). Both land in
            // the same auto-merge decision (anything other than `high` stays
            // for human review); only the displayed label differs.
            let fallback = if trigger == "budget_exhausted" {
                "low"
            } else {
                "mid"
            };
            warn!(
                module = "captain",
                item_id = item.id,
                confidence = ?out.confidence,
                trigger,
                fallback,
                "ship verdict missing or invalid confidence; defaulting (no auto-merge)"
            );
            out.confidence = Some(fallback.into());
        }
        if out
            .confidence_reason
            .as_deref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
        {
            out.confidence_reason =
                Some("confidence_reason missing — check evidence manually".into());
        }
        out
    } else {
        // Non-ship verdicts never carry confidence.
        CaptainVerdict {
            confidence: None,
            confidence_reason: None,
            ..verdict
        }
    }
}
