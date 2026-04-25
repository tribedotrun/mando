//! Rework → Queued transitions — extracted from the tick dispatch phase.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::service::lifecycle;
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

/// Transition all Rework items to Queued, clearing worker + interaction fields.
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
        if let Err(e) = lifecycle::apply_transition(item, ItemStatus::Queued) {
            tracing::error!(
                module = "captain",
                item_id = item.id,
                error = %e,
                "illegal rework transition"
            );
            continue;
        }
        item.worker = None;
        // worktree and workbench_id are permanent — rework reuses the same
        // worktree directory and workbench, only creating a new branch.
        item.branch = None;
        item.pr_number = None;
        item.worker_started_at = None;
        item.session_ids.worker = None;
        super::clear_task_interaction_sessions(item);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_rework_to_queued_clears_advisor_session() {
        let mut item = Task::new("rework");
        item.set_status_for_tests(ItemStatus::Rework);
        item.worker = Some("worker".into());
        item.branch = Some("feat/rework".into());
        item.pr_number = Some(42);
        item.session_ids.worker = Some("worker-sid".into());
        item.session_ids.ask = Some("ask-sid".into());
        item.session_ids.advisor = Some("advisor-sid".into());

        let mut alerts = Vec::new();
        transition_rework_to_queued(std::slice::from_mut(&mut item), &mut alerts);

        assert_eq!(item.status(), ItemStatus::Queued);
        assert!(item.session_ids.worker.is_none());
        assert!(item.session_ids.ask.is_none());
        assert!(item.session_ids.advisor.is_none());
        assert!(alerts.is_empty());
    }
}
