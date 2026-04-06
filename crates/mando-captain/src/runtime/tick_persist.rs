//! Immediate persistence for mid-tick discoveries.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::io::task_store::TaskStore;

/// Consecutive-failure counter for discovered-PR persistence. An alert is
/// surfaced on the tick once this crosses [`FLUSH_PR_ALERT_THRESHOLD`].
static FLUSH_PR_FAILURES: AtomicU32 = AtomicU32::new(0);
const FLUSH_PR_ALERT_THRESHOLD: u32 = 3;

pub(crate) async fn flush_discovered_prs(
    items: &[mando_types::Task],
    pre_tick_snapshot: &std::collections::HashMap<i64, serde_json::Value>,
    store_lock: &Arc<RwLock<TaskStore>>,
    alerts: &mut Vec<String>,
) {
    let mut any_failure = false;

    for item in items {
        let pr = match &item.pr {
            Some(pr) => pr.clone(),
            None => continue,
        };

        let had_pr = pre_tick_snapshot
            .get(&item.id)
            .and_then(|snapshot| snapshot.get("pr"))
            .and_then(|v| v.as_str())
            .is_some_and(|pr| !pr.is_empty());
        if had_pr {
            continue;
        }

        let store = store_lock.write().await;
        match store
            .update(item.id, |t| {
                t.pr = Some(pr.clone());
            })
            .await
        {
            Ok(_) => {
                tracing::info!(
                    module = "captain",
                    id = item.id,
                    pr = %pr,
                    "persisted discovered PR"
                );
            }
            Err(e) => {
                any_failure = true;
                tracing::warn!(
                    module = "captain",
                    id = item.id,
                    pr = %pr,
                    error = %e,
                    "failed to persist discovered PR"
                );
            }
        }
    }

    if any_failure {
        let count = FLUSH_PR_FAILURES.fetch_add(1, Ordering::Relaxed) + 1;
        // Alert once, exactly when the counter crosses the threshold. Using
        // `>=` here would push a new alert on every subsequent failing tick
        // (counts 3, 4, 5, ...) and flood the tick's alerts vec with
        // duplicates. The operator only needs to know once that we crossed;
        // the counter stays incremented so a longer run is still visible via
        // logs/metrics.
        if count == FLUSH_PR_ALERT_THRESHOLD {
            alerts.push(format!(
                "discovered-PR persistence failing ({count} consecutive ticks)"
            ));
        }
    } else {
        // Reset on both success AND no-op ticks. Without the no-op reset,
        // two failures separated by many ticks with nothing to flush would
        // appear as "2 consecutive" failures and trip the alert early.
        FLUSH_PR_FAILURES.store(0, Ordering::Relaxed);
    }
}
