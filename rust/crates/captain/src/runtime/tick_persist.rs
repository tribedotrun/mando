//! Immediate persistence for mid-tick discoveries.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

use anyhow::{Context, Result};

use crate::io::task_store::{TaskSnapshotJson, TaskStore};
use crate::Task;

pub(crate) type TaskSnapshotMap = HashMap<i64, Task>;
pub(crate) type JsonSnapshotMap = HashMap<i64, TaskSnapshotJson>;

/// Build parallel typed + JSON snapshots of items for tick-end change detection.
/// Typed map is consumed by `flush_discovered_prs`; JSON map feeds the io-layer 3-way merge.
pub(crate) fn build_pre_tick_snapshots(
    items: &[Task],
) -> Result<(TaskSnapshotMap, JsonSnapshotMap)> {
    let mut typed = TaskSnapshotMap::with_capacity(items.len());
    let mut json = JsonSnapshotMap::with_capacity(items.len());
    for it in items {
        let snap = TaskSnapshotJson::from_task(it).with_context(|| {
            format!("tick pre-snapshot serialization failed for task {}", it.id)
        })?;
        typed.insert(it.id, it.clone());
        json.insert(it.id, snap);
    }
    Ok((typed, json))
}

/// Consecutive-failure counter for discovered-PR persistence. An alert is
/// surfaced on the tick once this crosses [`FLUSH_PR_ALERT_THRESHOLD`].
static FLUSH_PR_FAILURES: AtomicU32 = AtomicU32::new(0);
const FLUSH_PR_ALERT_THRESHOLD: u32 = 3;

#[tracing::instrument(skip_all)]
pub(crate) async fn flush_discovered_prs(
    items: &[Task],
    pre_tick_snapshot: &HashMap<i64, Task>,
    store_lock: &Arc<RwLock<TaskStore>>,
    alerts: &mut Vec<String>,
) {
    let mut any_failure = false;

    for item in items {
        let pr_num = match item.pr_number {
            Some(n) => n,
            None => continue,
        };

        let had_pr = pre_tick_snapshot
            .get(&item.id)
            .and_then(|snapshot| snapshot.pr_number)
            .is_some();
        if had_pr {
            continue;
        }

        let store = store_lock.write().await;
        match store
            .update(item.id, |t| {
                t.pr_number = Some(pr_num);
            })
            .await
        {
            Ok(_) => {
                tracing::info!(
                    module = "captain",
                    id = item.id,
                    pr_number = pr_num,
                    "persisted discovered PR"
                );
            }
            Err(e) => {
                any_failure = true;
                tracing::warn!(
                    module = "captain",
                    id = item.id,
                    pr_number = pr_num,
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
