//! Polling and validation for captain review sessions.

use tracing::warn;

use crate::Task;

use super::captain_review::CaptainVerdict;

fn is_verdict_allowed(trigger: &str, action: &str) -> bool {
    // Captain is the last line of defense -- it must solve problems, not punt
    // them. `retry_clarifier` is restricted to the clarifier-fail tier.
    // `escalate` is available everywhere: without it, broken-session wedges
    // (watchdog idle timeout, rate-limit starvation, repeated-nudge loops
    // where respawn hits the same wall) have no escape hatch and the task
    // respawns forever. Broken-session reviews are otherwise narrower than
    // the default tier: they may ship, respawn, or escalate, but must never
    // resume the same dead session in place. A downstream non-empty-report check in
    // `validate_verdict` keeps escalate from being used as a lazy dodge.
    match trigger {
        "clarifier_fail" => matches!(action, "retry_clarifier" | "escalate"),
        "spawn_fail" => matches!(action, "respawn" | "escalate"),
        "broken_session" => matches!(action, "ship" | "respawn" | "escalate"),
        _ => matches!(
            action,
            "ship" | "nudge" | "respawn" | "reset_budget" | "escalate"
        ),
    }
}

/// Check if a captain review has completed. Returns the verdict if done.
pub(crate) fn check_review(item: &Task) -> Option<CaptainVerdict> {
    let session_id = item.session_ids.review.as_deref()?;
    let output = match super::agent_runtime::poll_structured_session_output(
        item.provider,
        session_id,
    ) {
        super::agent_runtime::AgentSessionPoll::Pending
        | super::agent_runtime::AgentSessionPoll::Failed(_) => return None,
        super::agent_runtime::AgentSessionPoll::UnusableOutput(msg) => {
            warn!(module = "captain", %session_id, %msg, "captain review produced no extractable verdict");
            return Some(CaptainVerdict {
                action: "escalate".into(),
                feedback: format!("Captain review produced no extractable verdict: {msg}"),
                report: Some(format!(
                    "Captain review session completed but produced no extractable verdict. {msg}"
                )),
                confidence: None,
                confidence_reason: None,
            });
        }
        super::agent_runtime::AgentSessionPoll::Completed(output) => output,
    };

    match output {
        super::agent_runtime::AgentSessionOutput::Structured {
            value,
            fallback_text,
        } => match serde_json::from_value::<CaptainVerdict>(value.clone()) {
            Ok(verdict) => Some(validate_verdict(verdict, item)),
            Err(e) => {
                let raw_preview: String = value.to_string().chars().take(300).collect();
                warn!(module = "captain", %e, %session_id, raw = %raw_preview,
                    "structured_output present but failed to parse");
                if let Some(verdict_text) = fallback_text {
                    parse_review_text(&verdict_text, item)
                } else {
                    Some(CaptainVerdict {
                        action: "escalate".into(),
                        feedback: format!("Failed to parse structured review verdict: {e}"),
                        report: Some(format!(
                            "Structured captain review output was present but invalid JSON for the expected schema. Raw output (first 300 chars): {raw_preview}"
                        )),
                        confidence: None,
                        confidence_reason: None,
                    })
                }
            }
        },
        super::agent_runtime::AgentSessionOutput::Text(verdict_text) => {
            parse_review_text(&verdict_text, item)
        }
    }
}

fn parse_review_text(verdict_text: &str, item: &Task) -> Option<CaptainVerdict> {
    match serde_json::from_str::<CaptainVerdict>(verdict_text) {
        Ok(verdict) => Some(validate_verdict(verdict, item)),
        Err(e) => {
            warn!(module = "captain", %e,
                preview = &verdict_text[..verdict_text.floor_char_boundary(200)],
                "failed to parse captain review verdict, defaulting to escalate");
            Some(CaptainVerdict {
                action: "escalate".into(),
                feedback: format!("Failed to parse review verdict: {e}"),
                report: Some(format!(
                    "Captain review verdict could not be parsed as JSON. Raw text (first 200 chars): {}",
                    &verdict_text[..verdict_text.floor_char_boundary(200)]
                )),
                confidence: None,
                confidence_reason: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_review_text_error_report_has_normal_spacing() {
        let task = Task::new("review spacing");
        let verdict = parse_review_text("not json", &task).expect("invalid JSON escalates");
        let report = verdict.report.expect("escalate report");

        assert!(report.starts_with("Captain review verdict could not be parsed as JSON. Raw text"));
        assert!(
            !report.contains("JSON.  Raw"),
            "report should not contain collapsed indentation spaces: {report:?}"
        );
    }
}

/// Check if the async CC task wrote an error result to the stream file.
///
/// Returns the error message if a failure marker is present.
pub(crate) fn check_review_failed(item: &Task) -> Option<String> {
    let session_id = item.session_ids.review.as_deref()?;
    match super::agent_runtime::poll_structured_session_output(item.provider, session_id) {
        super::agent_runtime::AgentSessionPoll::Failed(msg) => {
            warn!(module = "captain", %session_id, %msg, "captain review async task failed");
            Some(msg)
        }
        _ => None,
    }
}

/// Validate a parsed verdict against the trigger's allowed actions.
///
/// Also normalizes confidence fields on `ship`:
/// - If `confidence` is missing or not one of high/mid, default to "mid"
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

    // Escalate must carry a non-empty report. The prompt says so but the
    // model sometimes skips it; without the report, the human gets an
    // "escalated" task with no context. Synthesize a placeholder from the
    // feedback field and log so the miss is visible, rather than silently
    // shipping an empty report.
    if verdict.action == "escalate" {
        let report_missing = verdict
            .report
            .as_deref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true);
        if report_missing {
            let feedback_snippet = verdict.feedback.trim();
            let synthesized = if feedback_snippet.is_empty() {
                format!(
                    "Captain escalated trigger '{trigger}' without filling the report field. \
                     No feedback was provided either. Manual triage required — check the \
                     worker's stream and recent timeline."
                )
            } else {
                format!(
                    "Captain escalated trigger '{trigger}' without a report. \
                     Feedback provided: {feedback_snippet}. \
                     Manual triage required."
                )
            };
            warn!(
                module = "captain",
                item_id = item.id,
                trigger,
                "escalate verdict missing report; synthesizing placeholder"
            );
            return CaptainVerdict {
                action: "escalate".into(),
                feedback: verdict.feedback,
                report: Some(synthesized),
                confidence: None,
                confidence_reason: None,
            };
        }
    }

    if verdict.action == "ship" {
        let mut out = verdict;
        let confidence_valid = matches!(out.confidence.as_deref(), Some("high") | Some("mid"));
        if !confidence_valid {
            // The enforced JSON schema offers exactly `high` / `mid`, and the
            // prompt grades against those two. Anything else — missing, or a
            // value the model invented outside the schema — coerces to `mid`:
            // the verdict still ships to AwaitingReview but skips auto-merge,
            // which is the safe read of "we don't actually know".
            warn!(
                module = "captain",
                item_id = item.id,
                confidence = ?out.confidence,
                trigger,
                "ship verdict missing or invalid confidence; defaulting to mid (no auto-merge)"
            );
            out.confidence = Some("mid".into());
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
