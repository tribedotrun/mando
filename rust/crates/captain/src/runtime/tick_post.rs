//! §5 POST — persist health, prune WAL, SSE, summary.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};

use global_bus::EventBus;
use tokio::sync::RwLock;

use crate::io::{health_store, health_store::HealthState, ops_log, task_store::TaskStore};
use crate::service::tick_logic;

/// Persist health state, prune stale WAL entries, flush notifications,
/// and publish SSE events. Called at the end of every non-dry-run tick.
///
/// `changed_task_ids` are the task ids that the tick actually mutated; one
/// typed `Tasks(Some(..))` event is emitted per id so renderer detail caches
/// (`tasks.feed`, `tasks.timeline`, `tasks.artifacts`)
/// invalidate without waiting for a remount. The bare `Tasks(None)` catch-all
/// only invalidates the list, leaving per-task caches stale.
#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all)]
pub(crate) async fn run_post_phase(
    dry_run: bool,
    health_path: &Path,
    health_state: &HealthState,
    removed_workers: &[String],
    notifier: &super::notify::Notifier,
    bus: Option<&EventBus>,
    affected_task_ids: &[i64],
    changed_task_ids: &[i64],
    store_lock: &Arc<RwLock<TaskStore>>,
) -> Result<()> {
    if !dry_run {
        let mut fresh = health_store::load_health_state(health_path)
            .with_context(|| format!("load health state from {}", health_path.display()))?;
        merge_health_state(&mut fresh, health_state, removed_workers);
        if let Err(e) = health_store::save_health_state(health_path, &fresh) {
            tracing::warn!(module = "captain", error = %e, "health state save failed — worker tracking may be stale");
        }

        // Prune stale WAL entries (older than 72 hours).
        let wal_path = ops_log::ops_log_path();
        let mut wal = ops_log::load_ops_log(&wal_path);
        ops_log::prune_stale(&mut wal, ops_log::STALE_AGE_SECS);
        ops_log::save_ops_log(&wal, &wal_path).with_context(|| {
            format!(
                "tick post phase: failed to save ops log at {}",
                wal_path.display()
            )
        })?;
    }

    // Flush batched notifications.
    notifier.flush_batch().await;

    // SSE publish — notify UI of state changes.
    if let Some(bus) = bus {
        if !dry_run {
            broadcast_changed_tasks(bus, store_lock, changed_task_ids).await;
        }
        bus.send(global_bus::BusPayload::Status(Some(
            api_types::StatusEventData {
                action: Some("tick".into()),
                affected_task_ids: Some(affected_task_ids.to_vec()),
            },
        )));
        bus.send(global_bus::BusPayload::Sessions(Some(
            api_types::SessionsEventData {
                affected_task_ids: Some(affected_task_ids.to_vec()),
            },
        )));
    }

    Ok(())
}

/// Emit one typed `Tasks(Some({action: "updated", item, id, cleared_by: None}))`
/// event per changed task id, using the freshly-persisted DB row so the rev
/// matches what readers will see. Mirrors `daemon::broadcast_task_update`.
///
/// A missing or unreadable row is logged and skipped — the corresponding
/// renderer query falls back to the next snapshot/refresh rather than getting
/// an `item: None` event that would dodge the rev guard.
async fn broadcast_changed_tasks(
    bus: &EventBus,
    store_lock: &Arc<RwLock<TaskStore>>,
    changed_task_ids: &[i64],
) {
    if changed_task_ids.is_empty() {
        return;
    }
    let store = store_lock.read().await;
    for &task_id in changed_task_ids {
        match store.find_by_id(task_id).await {
            Ok(Some(task)) => {
                let item: Option<api_types::TaskItem> = serde_json::to_value(&task)
                    .ok()
                    .and_then(|v| serde_json::from_value(v).ok());
                if item.is_none() {
                    tracing::warn!(
                        module = "captain-runtime-tick_post",
                        task_id,
                        "skipping tick task broadcast — api-types schema drift"
                    );
                    continue;
                }
                bus.send(global_bus::BusPayload::Tasks(Some(
                    api_types::TaskEventData {
                        action: Some("updated".into()),
                        item,
                        id: Some(task_id),
                        cleared_by: None,
                    },
                )));
            }
            Ok(None) => {
                tracing::debug!(
                    module = "captain-runtime-tick_post",
                    task_id,
                    "skipping tick task broadcast — task not found (likely just deleted)"
                );
            }
            Err(e) => {
                tracing::warn!(
                    module = "captain-runtime-tick_post",
                    task_id,
                    error = %e,
                    "skipping tick task broadcast — DB read failed"
                );
            }
        }
    }
}

/// Touch workbench `last_activity_at` for changed tasks and broadcast
/// updates so the sidebar reflects captain-side events.
#[tracing::instrument(skip_all)]
pub(crate) async fn touch_affected_workbenches(
    wb_ids: &[i64],
    store_lock: &std::sync::Arc<tokio::sync::RwLock<crate::io::task_store::TaskStore>>,
    bus: Option<&global_bus::EventBus>,
) {
    let pool = store_lock.read().await.pool().clone();
    for wb_id in wb_ids {
        match crate::io::queries::workbenches::touch_activity(&pool, *wb_id).await {
            Ok(true) => {
                if let Some(bus) = bus {
                    if let Ok(Some(wb)) =
                        crate::io::queries::workbenches::find_by_id(&pool, *wb_id).await
                    {
                        match crate::runtime::daemon::workbench_runtime::to_wire_workbench_item(&wb)
                        {
                            Ok(item) => {
                                bus.send(global_bus::BusPayload::Workbenches(Some(
                                    api_types::WorkbenchEventData {
                                        action: Some("updated".into()),
                                        item: Some(item),
                                    },
                                )));
                            }
                            Err(e) => {
                                // Fire-and-forget tick post phase. Skip the
                                // bus broadcast instead of emitting an
                                // `item: None` event — schema drift has
                                // to be surfaced, not papered over.
                                tracing::error!(
                                    module = "captain-runtime-tick_post",
                                    workbench_id = wb.id,
                                    error = %e,
                                    "skipping workbench bus broadcast — api-types schema drift"
                                );
                            }
                        }
                    }
                }
            }
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(module = "captain-runtime-tick_post", workbench_id = wb_id, error = %e, "tick: failed to touch workbench activity");
            }
        }
    }
}

/// Merge in-memory health state into the on-disk snapshot.
///
/// Only tick-owned fields (`cpu_time_s`, `cwd`) are overlaid from the
/// in-memory snapshot. All other fields (written to disk by §4 EXECUTE)
/// are preserved from the on-disk version. Workers explicitly removed
/// during the tick (orphan cleanup) are removed from the merged result;
/// other on-disk entries are preserved even if absent from in-memory,
/// because they may have been created during §4 (e.g. newly dispatched
/// workers).
pub(crate) fn merge_health_state(
    on_disk: &mut HealthState,
    in_memory: &HealthState,
    removed_workers: &[String],
) {
    const TICK_OWNED_FIELDS: &[&str] = &["cpu_time_s", "cwd"];

    for (worker, entry) in in_memory {
        if let Some(obj) = entry.as_object() {
            for (k, v) in obj {
                if TICK_OWNED_FIELDS.contains(&k.as_str()) {
                    health_store::set_health_field(on_disk, worker, k, v.clone());
                }
            }
        }
    }

    // Only remove workers that were explicitly cleaned up during the tick.
    for w in removed_workers {
        on_disk.remove(w);
    }
}

/// Reconcile stale sessions after a tick.
#[tracing::instrument(skip_all)]
pub(crate) async fn run_post_cleanup(
    dry_run: bool,
    store_lock: &std::sync::Arc<tokio::sync::RwLock<crate::io::task_store::TaskStore>>,
    workflow: &settings::CaptainWorkflow,
    alerts: &mut Vec<String>,
) {
    if dry_run {
        return;
    }
    // Reconcile stale "running" sessions against stream ground truth.
    {
        let store = store_lock.read().await;
        super::session_reconcile::reconcile_running_sessions(
            store.pool(),
            workflow.agent.stale_threshold_s,
            alerts,
        )
        .await;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_health(fields: &[(&str, &str, serde_json::Value)]) -> HealthState {
        let mut state = HealthState::new();
        for (worker, field, value) in fields {
            health_store::set_health_field(&mut state, worker, field, value.clone());
        }
        state
    }

    #[test]
    fn pending_ai_feedback_cleared_on_disk_survives_merge() {
        let in_memory = make_health(&[
            ("w1", "pending_ai_feedback", serde_json::json!("fix CI")),
            ("w1", "cpu_time_s", serde_json::json!(42.0)),
            ("w1", "pid", serde_json::json!(1234)),
        ]);
        let mut on_disk = make_health(&[
            ("w1", "pid", serde_json::json!(5678)),
            ("w1", "nudge_count", serde_json::json!(5)),
        ]);

        merge_health_state(&mut on_disk, &in_memory, &[]);

        let cpu = health_store::get_health_f64(&on_disk, "w1", "cpu_time_s");
        assert_eq!(cpu, Some(42.0));
        let fb = health_store::get_health_str(&on_disk, "w1", "pending_ai_feedback");
        assert!(fb.is_none(), "pending_ai_feedback was clobbered: {fb:?}");
        let pid = health_store::get_health_u32(&on_disk, "w1", "pid");
        assert_eq!(pid, 5678);
        let nc = health_store::get_health_u32(&on_disk, "w1", "nudge_count");
        assert_eq!(nc, 5);
    }

    #[test]
    fn nudge_reason_fields_not_clobbered() {
        let in_memory = make_health(&[
            ("w1", "last_nudge_reason", serde_json::json!("old reason")),
            ("w1", "nudge_reason_consecutive", serde_json::json!(1)),
        ]);
        let mut on_disk = make_health(&[
            ("w1", "last_nudge_reason", serde_json::json!("new reason")),
            ("w1", "nudge_reason_consecutive", serde_json::json!(3)),
        ]);

        merge_health_state(&mut on_disk, &in_memory, &[]);

        let reason = health_store::get_health_str(&on_disk, "w1", "last_nudge_reason");
        assert_eq!(reason.as_deref(), Some("new reason"));
        let consec = health_store::get_health_u32(&on_disk, "w1", "nudge_reason_consecutive");
        assert_eq!(consec, 3);
    }

    #[test]
    fn orphan_worker_removed_from_merged_state() {
        let in_memory = make_health(&[("w1", "cpu_time_s", serde_json::json!(10.0))]);
        let mut on_disk = make_health(&[
            ("w1", "pid", serde_json::json!(1111)),
            ("orphan", "pid", serde_json::json!(9999)),
        ]);
        let removed = vec!["orphan".to_string()];

        merge_health_state(&mut on_disk, &in_memory, &removed);

        assert!(on_disk.contains_key("w1"), "live worker should survive");
        assert!(
            !on_disk.contains_key("orphan"),
            "orphan worker should be removed"
        );
    }

    #[test]
    fn new_worker_on_disk_preserved() {
        // A worker written to disk during §4 (dispatch) that wasn't in the
        // §1 snapshot should survive the merge.
        let in_memory = make_health(&[("w1", "cpu_time_s", serde_json::json!(10.0))]);
        let mut on_disk = make_health(&[
            ("w1", "pid", serde_json::json!(1111)),
            ("w2-new", "pid", serde_json::json!(2222)),
        ]);

        merge_health_state(&mut on_disk, &in_memory, &[]);

        assert!(on_disk.contains_key("w1"));
        assert!(
            on_disk.contains_key("w2-new"),
            "new worker written during §4 should be preserved"
        );
    }

    #[test]
    fn cwd_is_overlaid_from_in_memory() {
        let in_memory = make_health(&[("w1", "cwd", serde_json::json!("/new/path"))]);
        let mut on_disk = make_health(&[("w1", "cwd", serde_json::json!("/old/path"))]);

        merge_health_state(&mut on_disk, &in_memory, &[]);

        let cwd = health_store::get_health_str(&on_disk, "w1", "cwd");
        assert_eq!(cwd.as_deref(), Some("/new/path"));
    }

    /// Regression for the bug where the captain tick emitted only `Tasks(None)`
    /// after every tick — list-only invalidation that left per-task feed,
    /// timeline, and artifacts caches stale until the user
    /// remounted the workbench page. This test pins the typed broadcast.
    #[tokio::test]
    async fn broadcast_changed_tasks_emits_typed_event_per_id() {
        let db = global_db::Db::open_in_memory().await.unwrap();
        let pool = db.pool().clone();
        settings::projects::upsert(&pool, "test", "", None)
            .await
            .unwrap();
        let wb_id = crate::io::test_support::seed_workbench(&pool, 1).await;

        let mut task = crate::Task::new("watch me");
        task.project_id = 1;
        task.project = "test".into();
        task.workbench_id = wb_id;
        let task_id = crate::io::queries::tasks::insert_task(&pool, &task)
            .await
            .unwrap();

        let store_lock = std::sync::Arc::new(tokio::sync::RwLock::new(
            crate::io::task_store::TaskStore::new(pool),
        ));
        let bus = global_bus::EventBus::new();
        let mut rx = bus.subscribe();

        broadcast_changed_tasks(&bus, &store_lock, &[task_id]).await;

        let payload = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
            .await
            .expect("bus event must arrive within 1s")
            .expect("bus channel must yield a payload");
        let global_bus::BusPayload::Tasks(Some(data)) = payload else {
            panic!("expected typed Tasks(Some(..)) event, got {payload:?}");
        };
        assert_eq!(data.action.as_deref(), Some("updated"));
        assert_eq!(data.id, Some(task_id));
        assert!(data.cleared_by.is_none());
        let item = data
            .item
            .expect("item must be present so list cache can patch");
        assert_eq!(item.id, task_id);
        assert_eq!(item.title, "watch me");
    }

    #[tokio::test]
    async fn broadcast_changed_tasks_skips_missing_rows_silently() {
        let db = global_db::Db::open_in_memory().await.unwrap();
        let pool = db.pool().clone();
        let store_lock = std::sync::Arc::new(tokio::sync::RwLock::new(
            crate::io::task_store::TaskStore::new(pool),
        ));
        let bus = global_bus::EventBus::new();
        let mut rx = bus.subscribe();

        broadcast_changed_tasks(&bus, &store_lock, &[9999]).await;

        let recv = tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await;
        assert!(
            recv.is_err(),
            "expected no bus event for missing task, got {recv:?}"
        );
    }

    /// End-to-end guard that `run_post_phase` actually wires `changed_task_ids`
    /// into the bus broadcast. Catches a regression where someone keeps the
    /// helper but stops calling it from the post phase.
    #[tokio::test]
    async fn run_post_phase_emits_typed_task_event_for_changed_id() {
        let db = global_db::Db::open_in_memory().await.unwrap();
        let pool = db.pool().clone();
        settings::projects::upsert(&pool, "test", "", None)
            .await
            .unwrap();
        let wb_id = crate::io::test_support::seed_workbench(&pool, 1).await;

        let mut task = crate::Task::new("post-phase typed event");
        task.project_id = 1;
        task.project = "test".into();
        task.workbench_id = wb_id;
        let task_id = crate::io::queries::tasks::insert_task(&pool, &task)
            .await
            .unwrap();

        let store_lock = std::sync::Arc::new(tokio::sync::RwLock::new(
            crate::io::task_store::TaskStore::new(pool),
        ));
        let bus = global_bus::EventBus::new();
        let mut rx = bus.subscribe();
        let notifier = super::super::notify::Notifier::new(std::sync::Arc::new(bus.clone()));

        // Health-state bookkeeping is best-effort; point both reads/writes at
        // a unique temp file so concurrent tests don't collide.
        let health_path = std::env::temp_dir().join(format!(
            "tick_post_run_post_phase_{}.json",
            global_infra::uuid::Uuid::v4()
        ));
        std::fs::write(&health_path, "{}").unwrap();
        let health_state = HealthState::new();

        run_post_phase(
            false,
            &health_path,
            &health_state,
            &[],
            &notifier,
            Some(&bus),
            &[task_id],
            &[task_id],
            &store_lock,
        )
        .await
        .unwrap();
        let _ = std::fs::remove_file(&health_path);

        // Drain events; the typed Tasks(Some(..)) for our task id must be among them.
        let mut found_typed = false;
        for _ in 0..10 {
            match tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await {
                Ok(Ok(global_bus::BusPayload::Tasks(Some(data))))
                    if data.id == Some(task_id) && data.action.as_deref() == Some("updated") =>
                {
                    found_typed = true;
                    break;
                }
                Ok(Ok(_)) => continue,
                _ => break,
            }
        }
        assert!(
            found_typed,
            "run_post_phase must emit a typed Tasks(Some(..)) event for each changed task id"
        );
    }

    #[tokio::test]
    async fn broadcast_changed_tasks_no_op_on_empty_slice() {
        let db = global_db::Db::open_in_memory().await.unwrap();
        let pool = db.pool().clone();
        let store_lock = std::sync::Arc::new(tokio::sync::RwLock::new(
            crate::io::task_store::TaskStore::new(pool),
        ));
        let bus = global_bus::EventBus::new();
        let mut rx = bus.subscribe();

        broadcast_changed_tasks(&bus, &store_lock, &[]).await;

        let recv = tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await;
        assert!(recv.is_err(), "empty changed list must not emit any event");
    }
}
