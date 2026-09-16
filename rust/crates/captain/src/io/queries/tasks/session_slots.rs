//! The compare-and-swap write that keeps `tasks.session_ids` pointing at the
//! session a detached CC run is actually using.
//!
//! Lives apart from the rest of `tasks` because it is the one task write with
//! a concurrency contract of its own: it races the captain tick's end-of-tick
//! 3-way merge, and both sides now reconcile per slot.

use anyhow::Result;
use sqlx::SqlitePool;

/// Re-point one of a task's session ids at the session that is actually
/// running. A retried `CcOneShot` drops the pre-allocated id and CC mints
/// its own, so the poller would otherwise watch a stream file nothing
/// writes to and the task would ride out its full timeout.
///
/// Compare-and-swap on `expected_session_id`: the slot is rewritten only
/// while it still holds the id this run last pointed it at. Anything else
/// means the slot moved on — the phase finished and cleared it, a human
/// reopened the task, a newer attempt already re-pointed it — and a blind
/// write would resurrect a dead session id. A mismatch is a logged no-op,
/// not an error: the caller is a fire-and-forget spawn hook.
pub async fn retarget_session_id(
    pool: &SqlitePool,
    task_id: i64,
    slot: crate::SessionSlot,
    expected_session_id: &str,
    session_id: &str,
) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let stored: Option<String> = sqlx::query_scalar("SELECT session_ids FROM tasks WHERE id = ?1")
        .bind(task_id)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(stored) = stored else {
        return Ok(false);
    };
    let mut ids = crate::SessionIds::from_json(&stored)
        .map_err(|e| anyhow::anyhow!("invalid session_ids for task {task_id}: {e}"))?;
    if ids.get(slot) == Some(session_id) {
        return Ok(false);
    }
    if ids.get(slot) != Some(expected_session_id) {
        tracing::info!(
            module = "captain",
            task_id,
            expected_session_id,
            session_id,
            stored_session_id = ids.get(slot).unwrap_or("<none>"),
            "session slot moved on since this run claimed it; skipping retarget"
        );
        return Ok(false);
    }
    ids.set(slot, session_id.to_string());
    let result = sqlx::query("UPDATE tasks SET session_ids = ?1, rev = rev + 1 WHERE id = ?2")
        .bind(ids.to_json())
        .bind(task_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(result.rows_affected() > 0)
}
