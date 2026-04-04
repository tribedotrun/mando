//! Rework → Queued transitions — extracted from the tick dispatch phase.

use mando_types::task::ItemStatus;
use mando_types::Task;

/// Transition all Rework items to Queued, clearing worker fields.
pub(super) fn transition_rework_to_queued(items: &mut [Task]) {
    for item in items.iter_mut() {
        if item.status != ItemStatus::Rework {
            continue;
        }
        let item_id = item.id.to_string();
        let _lock = if item.id > 0 {
            match crate::io::item_lock::acquire_item_lock(&item_id, "tick-rework-dispatch") {
                Ok(lock) => Some(lock),
                Err(e) => {
                    tracing::info!(
                        module = "captain",
                        item_id = %item_id,
                        error = %e,
                        "skipping rework dispatch: item locked"
                    );
                    continue;
                }
            }
        } else {
            None
        };
        item.status = ItemStatus::Queued;
        item.worker = None;
        item.worktree = None;
        item.branch = None;
        item.pr = None;
        item.worker_started_at = None;
        item.session_ids.worker = None;
        item.session_ids.ask = None;
        tracing::info!(
            module = "captain",
            title = %&item.title[..item.title.len().min(60)],
            "dispatch: rework to queued"
        );
    }
}
