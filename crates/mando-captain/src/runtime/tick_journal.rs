//! Journal integration helpers for the captain tick loop.
//!
//! Extracted to keep `tick.rs` under the 500-line limit.

use mando_types::captain::{Action, ActionKind};
use mando_types::WorkerContext;

use crate::io::health_store::{self, HealthState};
use crate::io::journal::JournalDb;
use crate::io::journal_types::{DecisionInput, DecisionSource, StateSnapshot};

pub(crate) async fn resolve_outcomes(
    jdb: &JournalDb,
    actions: &[Action],
    worker_contexts: &[WorkerContext],
) {
    for action in actions {
        let is_skip = action.action == ActionKind::Skip;
        if let Err(e) = jdb.resolve_outcomes(&action.worker, is_skip).await {
            tracing::error!(
                module = "captain",
                worker = %action.worker,
                error = %e,
                "journal resolve failed"
            );
        }
    }

    let active_workers: std::collections::HashSet<&str> = worker_contexts
        .iter()
        .map(|c| c.session_name.as_str())
        .collect();
    if let Ok(unresolved) = jdb.unresolved_workers().await {
        for worker in unresolved {
            if !active_workers.contains(worker.as_str()) {
                if let Err(e) = jdb.resolve_terminal(&worker).await {
                    tracing::error!(
                        module = "captain",
                        worker = %worker,
                        error = %e,
                        "journal terminal resolve failed"
                    );
                }
            }
        }
    }
}

pub(crate) async fn log_decisions(
    jdb: &JournalDb,
    tick_id: &str,
    actions: &[Action],
    worker_contexts: &[WorkerContext],
    items: &[mando_types::Task],
    health_state: &HealthState,
) {
    for action in actions {
        if action.action == ActionKind::Skip {
            continue;
        }
        let item_id = items
            .iter()
            .find(|it| it.worker.as_deref() == Some(&action.worker))
            .map(|it| it.id.to_string());
        let ctx = match worker_contexts
            .iter()
            .find(|c| c.session_name == action.worker)
        {
            Some(c) => c,
            None => continue,
        };
        let nudge_count =
            health_store::get_health_u32(health_state, &action.worker, "nudge_count") as i64;
        let snapshot = StateSnapshot::from_worker_context(ctx, nudge_count);
        let source = DecisionSource::Deterministic;
        let rule = action.reason.as_deref().unwrap_or("unknown");
        let action_str = serde_json::to_value(&action.action)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| format!("{:?}", action.action));
        if let Err(e) = jdb
            .log_decision(&DecisionInput {
                tick_id,
                worker: &action.worker,
                item_id: item_id.as_deref(),
                action: &action_str,
                source,
                rule,
                state: &snapshot,
            })
            .await
        {
            tracing::error!(
                module = "captain",
                error = %e,
                "journal log failed — decision not recorded"
            );
        }
    }
}
