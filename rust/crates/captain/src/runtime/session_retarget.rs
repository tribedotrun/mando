//! Keeps `task.session_ids` pointing at the session that is actually running.
//!
//! `CcOneShot::run_with_retry_pid_hook` clears the caller's pre-allocated
//! session id on every retry — CC would otherwise refuse the id as already in
//! use — so a retried review or merge writes its stream under an id the
//! poller has never heard of. Without this the poller watches an empty file
//! and the task rides out its full timeout (1200s for review, 1800s for
//! merge) before anything notices.

use sqlx::SqlitePool;
use tracing::{info, warn};

use crate::SessionSlot;

/// Persist `effective_sid` into `slot`, replacing `expected_sid`.
///
/// Fire-and-forget: the spawn hook is synchronous and must not block the CC
/// session. The write is a compare-and-swap on `expected_sid` so a slot that
/// moved on while this run was retrying is left alone rather than pointed
/// back at a session that is no longer the one anybody is watching.
pub(crate) fn retarget_session_id(
    pool: &SqlitePool,
    task_id: i64,
    slot: SessionSlot,
    expected_sid: &str,
    effective_sid: &str,
) {
    if expected_sid == effective_sid {
        return;
    }
    info!(
        module = "captain",
        task_id,
        expected_sid,
        effective_sid,
        "CC adopted a different session id (one-shot retry); re-pointing the poller"
    );
    let pool = pool.clone();
    let expected = expected_sid.to_string();
    let sid = effective_sid.to_string();
    tokio::spawn(async move {
        if let Err(e) =
            crate::io::queries::tasks::retarget_session_id(&pool, task_id, slot, &expected, &sid)
                .await
        {
            warn!(
                module = "captain",
                task_id,
                sid = %sid,
                error = %e,
                "failed to re-point task session id after a one-shot retry"
            );
        }
    });
}
