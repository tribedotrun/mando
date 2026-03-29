//! §5 POST — persist health, prune WAL/journal, SSE, summary.

use std::path::Path;

use anyhow::Result;

use mando_shared::EventBus;
use mando_types::BusEvent;

use crate::biz::tick_logic;
use crate::io::{health_store, health_store::HealthState, ops_log};

/// Persist health state, prune stale WAL/journal entries, flush notifications,
/// and publish SSE events. Called at the end of every non-dry-run tick.
pub(crate) async fn run_post_phase(
    dry_run: bool,
    health_path: &Path,
    health_state: &HealthState,
    journal_db: &Option<crate::io::journal::JournalDb>,
    notifier: &super::notify::Notifier,
    bus: Option<&EventBus>,
) -> Result<()> {
    if !dry_run {
        // Reload from disk and merge: persist_worker_pid writes PID to disk
        // during the execute phase, so the in-memory health_state is stale for
        // PID fields. Reload, then overlay our in-memory cpu/stale updates.
        let mut fresh = health_store::load_health_state(health_path);
        for (worker, entry) in health_state {
            if let Some(obj) = entry.as_object() {
                for (k, v) in obj {
                    // Skip fields written to disk during §4 — the in-memory
                    // values are stale (from §1 load).
                    if k == "pid" || k == "stream_size_at_spawn" {
                        continue;
                    }
                    health_store::set_health_field(&mut fresh, worker, k, v.clone());
                }
            }
        }
        if let Err(e) = health_store::save_health_state(health_path, &fresh) {
            tracing::warn!(module = "captain", error = %e, "health state save failed — worker tracking may be stale");
        }

        // Prune stale WAL entries (older than 72 hours).
        let wal_path = ops_log::ops_log_path();
        let mut wal = ops_log::load_ops_log(&wal_path);
        ops_log::prune_stale(&mut wal, ops_log::STALE_AGE_SECS);
        if let Err(e) = ops_log::save_ops_log(&wal, &wal_path) {
            tracing::warn!(module = "captain", error = %e, "ops log save failed — completed operations may replay on restart");
        }

        // Prune stale journal decisions (older than 90 days).
        if let Some(ref jdb) = journal_db {
            match jdb.prune(90).await {
                Ok(pruned) if pruned > 0 => {
                    tracing::info!(module = "captain", pruned, "pruned old journal decisions");
                }
                Err(e) => {
                    tracing::warn!(module = "captain", error = %e, "journal prune failed — old decisions will accumulate");
                }
                _ => {}
            }
        }
    }

    // Flush batched notifications.
    notifier.flush_batch().await;

    // SSE publish — notify UI of state changes.
    if let Some(bus) = bus {
        bus.send(BusEvent::Tasks, None);
        bus.send(
            BusEvent::Status,
            Some(serde_json::json!({"action": "tick"})),
        );
        bus.send(BusEvent::Sessions, None);
    }

    Ok(())
}

/// Build tick summary from status counts and log it.
pub(crate) fn log_tick_summary(
    status_counts: &std::collections::HashMap<String, usize>,
    active_workers: usize,
    alert_count: usize,
) {
    let summary = tick_logic::format_status_summary(status_counts);
    tracing::info!(
        module = "captain",
        active_workers = active_workers,
        tasks = %summary,
        alert_count = alert_count,
        "tick done"
    );
}
