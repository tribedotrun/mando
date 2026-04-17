//! Rework → Queued transitions — extracted from the tick dispatch phase.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::{ItemStatus, Task};

use super::dashboard::truncate_utf8;

/// Per-task count of consecutive ticks where rework dispatch was skipped due
/// to a held item lock. Crossing [`REWORK_LOCK_ALERT_THRESHOLD`] raises a
/// tick alert so a stuck lock cannot silently block a task forever.
static REWORK_LOCK_SKIPS: Mutex<Option<HashMap<String, u32>>> = Mutex::new(None);
const REWORK_LOCK_ALERT_THRESHOLD: u32 = 5;

fn with_skip_map<R>(f: impl FnOnce(&mut HashMap<String, u32>) -> R) -> R {
    let mut guard = REWORK_LOCK_SKIPS.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(HashMap::new))
}

/// Transition all Rework items to Queued, clearing worker fields.
pub(super) fn transition_rework_to_queued(items: &mut [Task], alerts: &mut Vec<String>) {
    for item in items.iter_mut() {
        if item.status != ItemStatus::Rework {
            continue;
        }
        let item_id = item.id.to_string();
        let _lock = if item.id > 0 {
            match crate::io::item_lock::acquire_item_lock(&item_id, "tick-rework-dispatch") {
                Ok(lock) => Some(lock),
                Err(e) => {
                    let count = with_skip_map(|m| {
                        let entry = m.entry(item_id.clone()).or_insert(0);
                        *entry += 1;
                        *entry
                    });
                    tracing::warn!(
                        module = "captain",
                        item_id = %item_id,
                        consecutive = count,
                        error = %e,
                        "skipping rework dispatch: item locked"
                    );
                    // Alert once, exactly when the counter crosses the
                    // threshold. Using `>=` here would flood the tick's
                    // alerts vec with a duplicate on every subsequent
                    // locked tick (counts 5, 6, 7, ...); the operator only
                    // needs to know once that the threshold was crossed.
                    // Same rationale as FLUSH_PR_FAILURES in tick_persist.rs.
                    if count == REWORK_LOCK_ALERT_THRESHOLD {
                        alerts.push(format!(
                            "rework dispatch blocked for task {item_id} ({count} consecutive ticks)"
                        ));
                    }
                    continue;
                }
            }
        } else {
            None
        };
        item.status = ItemStatus::Queued;
        item.worker = None;
        // worktree and workbench_id are permanent — rework reuses the same
        // worktree directory and workbench, only creating a new branch.
        item.branch = None;
        item.pr_number = None;
        item.worker_started_at = None;
        item.session_ids.worker = None;
        item.session_ids.ask = None;
        with_skip_map(|m| {
            m.remove(&item_id);
        });
        tracing::info!(
            module = "captain",
            title = %truncate_utf8(&item.title, 60),
            "dispatch: rework to queued"
        );
    }
}
