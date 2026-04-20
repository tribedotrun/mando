//! Timeline event payload tagged-union (moved out of models_wire.rs to
//! respect the 500-line/file limit). See the enum doc for the strict-wire
//! contract.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ── Timeline event payloads (replaces Value on TimelineEvent.data) ──────
//
// Tagged discriminated union: one variant per event kind, selected by the
// `event_type` tag. Each variant is `#[serde(deny_unknown_fields)]` so
// producer drift surfaces as a hard decode error at the query boundary
// rather than silently breaking the Feed. See PR #878 for context.
//
// Serialized wire shape (nested under `TimelineEvent.data`):
//
//     { "event_type": "awaiting_review", "action": "ship", "confidence": ... }
//
// PR #889 completed the strict-typing story: every field is non-`Option`.
// When multiple producers emit the same `event_type` with disjoint field
// subsets they split into separate variants (`worker_nudged` vs
// `worker_nudge_failed`, `canceled` vs `canceled_by_human`, etc.).
// Fields whose upstream is genuinely nullable (the LLM may omit a
// confidence rating, a review may predate worktree recording) use the
// sentinel empty string `""` rather than `Option<String>`. The
// mergeability tick compares `confidence == "high"` so sentinels never
// leak into logic. `devtools/scripts/check_no_option_in_timeline_payload.py`
// enforces the no-`Option` rule mechanically.

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ClarifierQuestionPayload {
    pub question: String,
    pub answer: Option<String>,
    pub self_answered: bool,
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "event_type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TimelineEventPayload {
    Created {
        source: String,
    },
    ClarifyStarted {
        session_id: String,
    },
    ClarifyQuestion {
        session_id: String,
        questions: Vec<ClarifierQuestionPayload>,
    },
    ClarifyTimeout {
        timeout_s: i64,
    },
    ClarifyResolved {
        session_id: String,
    },
    HumanAnswered {
        answer: String,
    },
    WorkerSpawned {
        worker: String,
        session_id: String,
    },
    PlanningSpawned {
        worker: String,
        session_id: String,
    },
    WorkerNudged {
        worker: String,
        session_id: String,
        content: String,
        reason: String,
        nudge_count: i64,
    },
    WorkerNudgeFailed {
        worker: String,
        session_id: String,
        reason: String,
        nudge_count_attempted: i64,
        error: String,
    },
    SessionResumed {
        worker: String,
        session_id: String,
    },
    WorkerCompleted {},
    CaptainReviewStarted {
        trigger: String,
        session_id: String,
    },
    CaptainReviewMergeFail {
        trigger: String,
        fail_count: i64,
        error: String,
        retries: i64,
    },
    CaptainReviewClarifierFail {
        trigger: String,
        fail_count: i64,
    },
    CaptainReviewCiFailure {
        trigger: String,
    },
    CaptainReviewRebaseExhausted {
        trigger: String,
        pr: String,
        retries: i64,
        reason: String,
    },
    CaptainReviewVerdict {
        action: String,
        feedback: String,
        confidence: String,
        confidence_reason: String,
        reviewed_head_sha: String,
    },
    CaptainReviewRetry {
        action: String,
        feedback: String,
        error: String,
        fail_count: i64,
    },
    CaptainMergeStarted {
        session_id: String,
        pr: String,
    },
    CaptainMergeQueued {
        pr: String,
        source: String,
        confidence_reason: String,
    },
    CaptainMergeRetry {
        error: String,
        fail_count: i64,
    },
    AutoMergeTriage {},
    AutoMergeTriageFailed {},
    AutoMergeTriageExhausted {},
    AwaitingReview {
        action: String,
        feedback: String,
        confidence: String,
        confidence_reason: String,
        reviewed_head_sha: String,
    },
    HumanReopen {
        content: String,
        worker: String,
        session_id: String,
        from: String,
        to: String,
        source: String,
    },
    HumanAsk {
        question: String,
        intent: String,
        ask_id: String,
    },
    RebaseTriggered {
        worker: String,
        session_id: String,
        pr: String,
        attempt: i64,
        max_retries: i64,
    },
    ReworkRequested {
        content: String,
        to: String,
    },
    Merged {
        pr: String,
        source: String,
        accepted_by: String,
    },
    AcceptedNoPr {
        accepted_by: String,
    },
    Escalated {
        action: String,
        feedback: String,
        confidence: String,
        confidence_reason: String,
        reviewed_head_sha: String,
    },
    ReviewErrored {
        error: String,
        fail_count: i64,
    },
    /// The clarifier turn failed at the CC transport layer (as opposed to a
    /// structured "escalate" verdict). Emitted both by the HTTP inline path
    /// (`answer_and_reclarify` error) and by the captain tick path
    /// (background clarifier revert). Renderer surfaces a "CC errored —
    /// retry" card distinct from stale `needs-clarification`.
    ///
    /// Sentinel encoding (per PR #889's no-`Option` rule):
    /// - `session_id == ""` — failure surfaced before a CC session was
    ///   established (spawn failure, pre-prompt timeout).
    /// - `api_error_status == 0` — the underlying error was not an HTTP
    ///   status (transport/internal panic, not an API response).
    ClarifierFailed {
        session_id: String,
        api_error_status: u16,
        message: String,
    },
    Canceled {
        pr: String,
    },
    CanceledByHuman {
        canceled_by: String,
    },
    HandedOff {
        to: String,
        handed_off_by: String,
    },
    CompletedNoPr {
        action: String,
        feedback: String,
        confidence: String,
        confidence_reason: String,
        reviewed_head_sha: String,
    },
    ClarifierCompletedNoPr {
        session_id: String,
    },
    StatusChanged {
        from: String,
        to: String,
    },
    StatusChangedByCommand {
        from: String,
        to: String,
        command: String,
    },
    StatusChangedQueued {
        to: String,
        reason: String,
    },
    StatusChangedRetryMerge {
        from: String,
        to: String,
        pr: i64,
    },
    StatusChangedClarifierFail {
        from: String,
        to: String,
        session_id: String,
        error: String,
    },
    RateLimited {
        remaining_secs: i64,
        retry_at: String,
    },
    RateLimitCleared {
        action: String,
        cleared_by: String,
    },
    WorkerReopened {
        source: String,
        reopen_seq: i64,
        outcome: String,
        feedback: String,
        worker: String,
        session_id: String,
    },
    HumanAskFailed {
        question: String,
        error: String,
    },
    EvidenceUpdated {},
    WorkSummaryUpdated {},
    PlanningRound {
        round: i64,
        cc_feedback_len: i64,
        codex_feedback_len: i64,
    },
    PlanCompleted {
        diagram: String,
        plan: String,
    },
    PlanReady {},
}

impl TimelineEventPayload {
    /// Serialize to JSON with the `event_type` tag stripped. The tag lives
    /// in the separate `timeline_events.event_type` column on disk, so the
    /// stored blob must not duplicate it (and the read-side `into_event`
    /// re-injects the column value before decoding). Every callsite that
    /// writes a `task.timeline.project` lifecycle effect must route through
    /// this helper — hand-rolling the `data` object risks writing shapes
    /// that fail to round-trip through the strict tagged decode.
    pub fn data_without_tag(&self) -> Result<serde_json::Value, serde_json::Error> {
        let mut value = serde_json::to_value(self)?;
        if let Some(obj) = value.as_object_mut() {
            obj.remove("event_type");
        }
        Ok(value)
    }

    /// Serde-tag string for this payload variant (matches the on-disk
    /// `timeline_events.event_type` column value).
    pub fn event_type_str(&self) -> &'static str {
        match self {
            Self::Created { .. } => "created",
            Self::ClarifyStarted { .. } => "clarify_started",
            Self::ClarifyQuestion { .. } => "clarify_question",
            Self::ClarifyTimeout { .. } => "clarify_timeout",
            Self::ClarifyResolved { .. } => "clarify_resolved",
            Self::HumanAnswered { .. } => "human_answered",
            Self::WorkerSpawned { .. } => "worker_spawned",
            Self::PlanningSpawned { .. } => "planning_spawned",
            Self::WorkerNudged { .. } => "worker_nudged",
            Self::WorkerNudgeFailed { .. } => "worker_nudge_failed",
            Self::SessionResumed { .. } => "session_resumed",
            Self::WorkerCompleted { .. } => "worker_completed",
            Self::CaptainReviewStarted { .. } => "captain_review_started",
            Self::CaptainReviewMergeFail { .. } => "captain_review_merge_fail",
            Self::CaptainReviewClarifierFail { .. } => "captain_review_clarifier_fail",
            Self::CaptainReviewCiFailure { .. } => "captain_review_ci_failure",
            Self::CaptainReviewRebaseExhausted { .. } => "captain_review_rebase_exhausted",
            Self::CaptainReviewVerdict { .. } => "captain_review_verdict",
            Self::CaptainReviewRetry { .. } => "captain_review_retry",
            Self::CaptainMergeStarted { .. } => "captain_merge_started",
            Self::CaptainMergeQueued { .. } => "captain_merge_queued",
            Self::CaptainMergeRetry { .. } => "captain_merge_retry",
            Self::AutoMergeTriage { .. } => "auto_merge_triage",
            Self::AutoMergeTriageFailed { .. } => "auto_merge_triage_failed",
            Self::AutoMergeTriageExhausted { .. } => "auto_merge_triage_exhausted",
            Self::AwaitingReview { .. } => "awaiting_review",
            Self::HumanReopen { .. } => "human_reopen",
            Self::HumanAsk { .. } => "human_ask",
            Self::RebaseTriggered { .. } => "rebase_triggered",
            Self::ReworkRequested { .. } => "rework_requested",
            Self::Merged { .. } => "merged",
            Self::AcceptedNoPr { .. } => "accepted_no_pr",
            Self::Escalated { .. } => "escalated",
            Self::ReviewErrored { .. } => "review_errored",
            Self::ClarifierFailed { .. } => "clarifier_failed",
            Self::Canceled { .. } => "canceled",
            Self::CanceledByHuman { .. } => "canceled_by_human",
            Self::HandedOff { .. } => "handed_off",
            Self::CompletedNoPr { .. } => "completed_no_pr",
            Self::ClarifierCompletedNoPr { .. } => "clarifier_completed_no_pr",
            Self::StatusChanged { .. } => "status_changed",
            Self::StatusChangedByCommand { .. } => "status_changed_by_command",
            Self::StatusChangedQueued { .. } => "status_changed_queued",
            Self::StatusChangedRetryMerge { .. } => "status_changed_retry_merge",
            Self::StatusChangedClarifierFail { .. } => "status_changed_clarifier_fail",
            Self::RateLimited { .. } => "rate_limited",
            Self::RateLimitCleared { .. } => "rate_limit_cleared",
            Self::WorkerReopened { .. } => "worker_reopened",
            Self::HumanAskFailed { .. } => "human_ask_failed",
            Self::EvidenceUpdated { .. } => "evidence_updated",
            Self::WorkSummaryUpdated { .. } => "work_summary_updated",
            Self::PlanningRound { .. } => "planning_round",
            Self::PlanCompleted { .. } => "plan_completed",
            Self::PlanReady { .. } => "plan_ready",
        }
    }
}
#[cfg(test)]
mod timeline_payload_roundtrip_tests {
    use super::*;

    // Regresses a class of bug where reopen routes hand-rolled the `data`
    // blob of a `task.timeline.project` effect instead of using
    // `data_without_tag()`. The hand-rolled shape could serialize
    // `worker: Option<String>::None` as JSON `null`, but the `WorkerSpawned`
    // variant declares `worker: String` (required) -- so the stored row
    // failed to round-trip through the strict tagged decoder and 500'd the
    // feed endpoint.
    //
    // Post-fix, callers must route through `.data_without_tag()` on the
    // typed payload; this test proves that helper produces a shape the
    // read-side injector + `from_value` can re-ingest cleanly.
    #[test]
    fn worker_spawned_roundtrips_via_data_without_tag() {
        let payload = TimelineEventPayload::WorkerSpawned {
            worker: "worker-1".to_string(),
            session_id: "s-abc".to_string(),
        };
        let mut stored = payload.data_without_tag().expect("strip tag");
        assert!(
            stored.get("event_type").is_none(),
            "tag must be stripped from stored blob: {stored}"
        );
        stored
            .as_object_mut()
            .unwrap()
            .insert("event_type".into(), serde_json::json!("worker_spawned"));
        let decoded: TimelineEventPayload =
            serde_json::from_value(stored).expect("round-trip through tagged decoder");
        match decoded {
            TimelineEventPayload::WorkerSpawned { worker, session_id } => {
                assert_eq!(worker, "worker-1");
                assert_eq!(session_id, "s-abc");
            }
            other => panic!("expected WorkerSpawned, got {other:?}"),
        }
    }

    // PR #886 + #889: ClarifierFailed must round-trip through
    // data_without_tag, with PR #889's no-Option convention applied
    // (sentinel "" for absent session_id, sentinel 0 for non-HTTP errors).
    #[test]
    fn clarifier_failed_roundtrips_via_data_without_tag() {
        let payload = TimelineEventPayload::ClarifierFailed {
            session_id: "sess-cf".to_string(),
            api_error_status: 400,
            message: "API Error: 400 bad_request".to_string(),
        };
        let mut stored = payload.data_without_tag().expect("strip tag");
        assert!(
            stored.get("event_type").is_none(),
            "tag must be stripped from stored blob: {stored}"
        );
        stored
            .as_object_mut()
            .unwrap()
            .insert("event_type".into(), serde_json::json!("clarifier_failed"));
        let decoded: TimelineEventPayload =
            serde_json::from_value(stored).expect("round-trip through tagged decoder");
        match decoded {
            TimelineEventPayload::ClarifierFailed {
                session_id,
                api_error_status,
                message,
            } => {
                assert_eq!(session_id, "sess-cf");
                assert_eq!(api_error_status, 400);
                assert_eq!(message, "API Error: 400 bad_request");
            }
            other => panic!("expected ClarifierFailed, got {other:?}"),
        }
    }

    // Pre-session failure (spawn failure, pre-prompt timeout): both
    // session_id and api_error_status use their sentinel values.
    #[test]
    fn clarifier_failed_roundtrips_with_sentinel_absent_fields() {
        let payload = TimelineEventPayload::ClarifierFailed {
            session_id: String::new(),
            api_error_status: 0,
            message: "spawn failed".to_string(),
        };
        let mut stored = payload.data_without_tag().expect("strip tag");
        stored
            .as_object_mut()
            .unwrap()
            .insert("event_type".into(), serde_json::json!("clarifier_failed"));
        let decoded: TimelineEventPayload =
            serde_json::from_value(stored).expect("round-trip through tagged decoder");
        match decoded {
            TimelineEventPayload::ClarifierFailed {
                session_id,
                api_error_status,
                message,
            } => {
                assert_eq!(session_id, "");
                assert_eq!(api_error_status, 0);
                assert_eq!(message, "spawn failed");
            }
            other => panic!("expected ClarifierFailed, got {other:?}"),
        }
    }

    // Fencing the exact pre-fix buggy shape: `{"worker": null, "session_id":
    // null}` paired with `event_type: "worker_spawned"`. The strict tagged
    // decoder must reject this, not silently coerce.
    #[test]
    fn worker_spawned_rejects_null_worker_field() {
        let buggy_blob = serde_json::json!({
            "event_type": "worker_spawned",
            "worker": serde_json::Value::Null,
            "session_id": serde_json::Value::Null,
        });
        let err = serde_json::from_value::<TimelineEventPayload>(buggy_blob)
            .expect_err("null worker must fail strict decode");
        let msg = err.to_string();
        assert!(
            msg.contains("null") && msg.contains("string"),
            "decode error should reject null-for-required-string: {msg}"
        );
    }
}
