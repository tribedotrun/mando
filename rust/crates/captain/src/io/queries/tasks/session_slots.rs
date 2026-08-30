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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::queries::tasks::{find_by_id, insert_task};
    use crate::{SessionSlot, Task};

    async fn test_pool() -> SqlitePool {
        let db = global_db::Db::open_in_memory().await.unwrap();
        settings::projects::upsert(db.pool(), "test", "", None)
            .await
            .unwrap();
        db.pool().clone()
    }

    async fn test_task(pool: &SqlitePool) -> Task {
        let mut task = Task::new("retarget");
        task.project_id = 1;
        task.project = "test".into();
        task.workbench_id = crate::io::test_support::seed_workbench(pool, 1).await;
        task
    }

    /// A retried one-shot writes its stream under an id CC minted itself.
    /// The poller reads `session_ids.review`, so that slot has to follow.
    #[tokio::test]
    async fn retarget_points_the_slot_at_the_running_session() {
        let pool = test_pool().await;
        let mut task = test_task(&pool).await;
        task.session_ids.review = Some("preallocated".into());
        let id = insert_task(&pool, &task).await.unwrap();

        assert!(
            retarget_session_id(&pool, id, SessionSlot::Review, "preallocated", "cc-minted")
                .await
                .unwrap()
        );

        let reloaded = find_by_id(&pool, id).await.unwrap().unwrap();
        assert_eq!(reloaded.session_ids.review.as_deref(), Some("cc-minted"));
    }

    #[tokio::test]
    async fn retarget_leaves_other_slots_alone() {
        let pool = test_pool().await;
        let mut task = test_task(&pool).await;
        task.session_ids.review = Some("review-sid".into());
        task.session_ids.worker = Some("worker-sid".into());
        task.session_ids.merge = Some("preallocated".into());
        let id = insert_task(&pool, &task).await.unwrap();

        retarget_session_id(&pool, id, SessionSlot::Merge, "preallocated", "merge-sid")
            .await
            .unwrap();

        let reloaded = find_by_id(&pool, id).await.unwrap().unwrap();
        assert_eq!(reloaded.session_ids.merge.as_deref(), Some("merge-sid"));
        assert_eq!(reloaded.session_ids.review.as_deref(), Some("review-sid"));
        assert_eq!(reloaded.session_ids.worker.as_deref(), Some("worker-sid"));
    }

    #[tokio::test]
    async fn retarget_is_a_noop_when_the_slot_already_matches() {
        let pool = test_pool().await;
        let mut task = test_task(&pool).await;
        task.session_ids.merge = Some("same".into());
        let id = insert_task(&pool, &task).await.unwrap();
        let before = find_by_id(&pool, id).await.unwrap().unwrap().rev;

        assert!(
            !retarget_session_id(&pool, id, SessionSlot::Merge, "same", "same")
                .await
                .unwrap()
        );

        let after = find_by_id(&pool, id).await.unwrap().unwrap().rev;
        assert_eq!(before, after, "a no-op must not bump rev");
    }

    /// The retarget is fire-and-forget, so it can land after the slot has
    /// already moved on. Writing blind there would resurrect a dead id — the
    /// compare-and-swap makes it a no-op instead.
    #[tokio::test]
    async fn retarget_no_ops_when_the_slot_holds_a_different_id() {
        let pool = test_pool().await;
        let mut task = test_task(&pool).await;
        task.session_ids.merge = Some("someone-elses-sid".into());
        let id = insert_task(&pool, &task).await.unwrap();
        let before = find_by_id(&pool, id).await.unwrap().unwrap().rev;

        assert!(
            !retarget_session_id(&pool, id, SessionSlot::Merge, "preallocated", "cc-minted")
                .await
                .unwrap()
        );

        let reloaded = find_by_id(&pool, id).await.unwrap().unwrap();
        assert_eq!(
            reloaded.session_ids.merge.as_deref(),
            Some("someone-elses-sid")
        );
        assert_eq!(before, reloaded.rev, "a no-op must not bump rev");
    }

    /// A cleared slot (the phase finished while the retry was in flight) is
    /// also a mismatch — the retarget must not re-populate it.
    #[tokio::test]
    async fn retarget_no_ops_when_the_slot_was_cleared() {
        let pool = test_pool().await;
        let task = test_task(&pool).await;
        let id = insert_task(&pool, &task).await.unwrap();

        assert!(
            !retarget_session_id(&pool, id, SessionSlot::Merge, "preallocated", "cc-minted")
                .await
                .unwrap()
        );

        let reloaded = find_by_id(&pool, id).await.unwrap().unwrap();
        assert!(reloaded.session_ids.merge.is_none());
    }

    #[tokio::test]
    async fn retarget_reports_a_missing_task() {
        let pool = test_pool().await;
        assert!(
            !retarget_session_id(&pool, 4242, SessionSlot::Review, "preallocated", "sid")
                .await
                .unwrap()
        );
    }
}
