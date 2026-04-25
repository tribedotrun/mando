//! Small builders for captain-review timeline payloads. Factored out of
//! `captain_review_verdict.rs` so the match arms there stay focused on
//! lifecycle transitions, not payload shape.

use crate::TimelineEventPayload;

/// Sentinel when `reviewed_head_sha` is unavailable (worktree missing,
/// `git rev-parse` failed). The mergeability auto-merge gate compares
/// against the PR head SHA; any mismatch skips auto-merge and leaves
/// the task for human review, which is the correct behavior in both
/// the "stale verdict" and "no SHA recorded" cases.
pub(super) const UNKNOWN_SHA: &str = "unknown";

/// Read `git rev-parse HEAD` from a worktree. Returns the
/// `UNKNOWN_SHA` sentinel if the worktree path is missing, the command
/// errors, or the output isn't a valid SHA-1 or SHA-256 hex string.
/// Used to stamp ship verdicts with the reviewed head so the
/// mergeability tick can detect post-review pushes.
#[tracing::instrument(skip_all)]
pub(super) async fn read_worktree_head_sha(worktree: Option<&str>) -> String {
    let Some(wt) = worktree else {
        return UNKNOWN_SHA.to_string();
    };
    let wt_path = global_infra::paths::expand_tilde(wt);
    let Ok(sha) = global_git::head_sha(&wt_path).await else {
        return UNKNOWN_SHA.to_string();
    };
    if !matches!(sha.len(), 40 | 64) || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return UNKNOWN_SHA.to_string();
    }
    sha
}

/// Build a `CaptainReviewVerdict` timeline payload. Upstream `confidence`
/// / `confidence_reason` are genuinely optional from the LLM; when
/// absent the empty-string sentinel survives round-trip through the
/// strict tagged decoder. The mergeability gate checks
/// `confidence == "high"` so the sentinel naturally falls through.
pub(super) fn verdict_payload(
    action: &str,
    feedback: &str,
    confidence: Option<&str>,
    confidence_reason: Option<&str>,
    reviewed_head_sha: &str,
) -> TimelineEventPayload {
    TimelineEventPayload::CaptainReviewVerdict {
        action: action.to_string(),
        feedback: feedback.to_string(),
        confidence: confidence.unwrap_or("").to_string(),
        confidence_reason: confidence_reason.unwrap_or("").to_string(),
        reviewed_head_sha: reviewed_head_sha.to_string(),
    }
}

/// Build an `Escalated` timeline payload with the same five verdict-shape
/// fields that `CaptainReviewVerdict` carries.
pub(super) fn escalated_payload(
    action: &str,
    feedback: &str,
    confidence: Option<&str>,
    confidence_reason: Option<&str>,
    reviewed_head_sha: &str,
) -> TimelineEventPayload {
    TimelineEventPayload::Escalated {
        action: action.to_string(),
        feedback: feedback.to_string(),
        confidence: confidence.unwrap_or("").to_string(),
        confidence_reason: confidence_reason.unwrap_or("").to_string(),
        reviewed_head_sha: reviewed_head_sha.to_string(),
    }
}

/// Ship verdict splits into `CompletedNoPr` (no_pr item) vs `AwaitingReview`
/// (normal PR item); the five payload fields are identical across both.
pub(super) fn ship_payload(
    is_no_pr: bool,
    action: &str,
    feedback: &str,
    confidence: Option<&str>,
    confidence_reason: Option<&str>,
    reviewed_head_sha: &str,
) -> TimelineEventPayload {
    let action = action.to_string();
    let feedback = feedback.to_string();
    let confidence = confidence.unwrap_or("").to_string();
    let confidence_reason = confidence_reason.unwrap_or("").to_string();
    let reviewed_head_sha = reviewed_head_sha.to_string();
    if is_no_pr {
        TimelineEventPayload::CompletedNoPr {
            action,
            feedback,
            confidence,
            confidence_reason,
            reviewed_head_sha,
        }
    } else {
        TimelineEventPayload::AwaitingReview {
            action,
            feedback,
            confidence,
            confidence_reason,
            reviewed_head_sha,
        }
    }
}
