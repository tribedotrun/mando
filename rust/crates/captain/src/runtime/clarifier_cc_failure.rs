//! PR #886 helpers pulled out of `clarifier.rs` to keep it under the
//! 500-line budget. Used by the HTTP clarify path to (a) build the
//! interactive re-clarify JSON schema and (b) record a cc_sessions row
//! for a failed re-clarify turn so the UI's retry card has context.

use tracing::warn;

use crate::Task;

/// JSON schema for the interactive re-clarify turn (multi-turn with
/// human answer). Distinct from the initial clarifier schema — this one
/// carries the `status` enum used to decide readiness after a human
/// answer.
pub(super) fn build_interactive_clarifier_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "status": { "type": "string", "enum": ["understood", "ready", "clarifying", "escalate"] },
            "context": { "type": "string" },
            "title": { "type": "string" },
            "no_pr": { "type": ["boolean", "null"] },
            "resource": { "type": ["string", "null"] },
            "questions": {
                "type": ["array", "null"],
                "items": {
                    "type": "object",
                    "properties": {
                        "question": { "type": "string" },
                        "answer": { "type": ["string", "null"] },
                        "self_answered": { "type": "boolean" },
                        "category": { "type": "string", "enum": ["code", "intent"] }
                    },
                    "required": ["question", "self_answered", "category"]
                }
            }
        },
        "required": ["status", "context", "title"]
    })
}

/// Log a cc_sessions row for a failed re-clarify CC turn so the UI's
/// retry card can see what went wrong. Called from the error arm of
/// `answer_and_reclarify` before the error propagates up.
#[tracing::instrument(skip_all)]
pub(super) async fn log_reclarify_failure(
    pool: &sqlx::SqlitePool,
    item: &Task,
    cwd: &std::path::Path,
    e: &global_claude::CcError,
) {
    let (session_id, api_error_status) = match e {
        global_claude::CcError::ApiError {
            session_id,
            api_error_status,
            ..
        } => (session_id.clone(), api_error_status.map(i64::from)),
        _ => (item.session_ids.clarifier.clone().unwrap_or_default(), None),
    };
    if let Err(log_err) = crate::io::headless_cc::log_cc_failure(
        pool,
        &session_id,
        cwd,
        "clarifier",
        Some(item.id),
        Some(&format!("{e}")),
        api_error_status,
    )
    .await
    {
        warn!(module = "clarifier", error = %log_err, "failed to log clarifier CC failure");
    }
}
