//! Immediate persistence for mid-tick discoveries.

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::io::task_store::TaskStore;

pub(crate) async fn flush_discovered_prs(
    items: &[mando_types::Task],
    pre_tick_snapshot: &std::collections::HashMap<i64, serde_json::Value>,
    store_lock: &Arc<RwLock<TaskStore>>,
) {
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
        if let Err(e) = store
            .update(item.id, |t| {
                t.pr = Some(pr.clone());
            })
            .await
        {
            tracing::warn!(
                module = "captain",
                id = item.id,
                pr = %pr,
                error = %e,
                "failed to persist discovered PR"
            );
        } else {
            tracing::info!(
                module = "captain",
                id = item.id,
                pr = %pr,
                "persisted discovered PR"
            );
        }
    }
}
