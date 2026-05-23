use anyhow::{Context, Result};

/// Mark a clarifier session's result as applied to its parent task.
///
/// Idempotency guardrail: once this is set, `tick_clarify_poll` skips the
/// session even if the task re-enters `Clarifying` (e.g. the human answered
/// a follow-up question and the HTTP inline path preserved the old
/// session id). Without this, the poll would re-apply the prior round's
/// already-consumed stream on top of the fresh human answer.
///
/// Safe to call multiple times: subsequent calls are no-ops because the
/// column's first write wins. No-op if the session doesn't exist.
pub async fn mark_session_result_applied(pool: &sqlx::SqlitePool, session_id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE cc_sessions
         SET result_applied_at = ?1
         WHERE session_id = ?2 AND result_applied_at IS NULL",
    )
    .bind(global_types::now_rfc3339())
    .bind(session_id)
    .execute(pool)
    .await
    .context("mark_session_result_applied")?;
    Ok(())
}

/// Transaction variant of `mark_session_result_applied` for callers that
/// want the marker write to land in the same transaction as the task
/// transition.
pub async fn mark_session_result_applied_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    session_id: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE cc_sessions
         SET result_applied_at = ?1
         WHERE session_id = ?2 AND result_applied_at IS NULL",
    )
    .bind(global_types::now_rfc3339())
    .bind(session_id)
    .execute(&mut **tx)
    .await
    .context("mark_session_result_applied_in_tx")?;
    Ok(())
}
