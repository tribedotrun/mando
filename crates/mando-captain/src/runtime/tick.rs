//! Captain tick entry point — `run_captain_tick()`.
//!
//! 5-phase single-pass tick:
//! §1 LOAD — all non-terminal items + health state + Linear sync + kill orphans
//! §2 GATHER — context for ALL non-terminal items
//! §3 CLASSIFY — one pass, all items, produces action list
//! §4 EXECUTE — all actions
//! §5 POST — persist, SSE, prune

use std::sync::Arc;

use anyhow::Result;
use tracing::Instrument;

use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_shared::EventBus;
use mando_types::captain::TickResult;
use mando_types::task::ItemStatus;
use tokio::sync::RwLock;

use crate::io::{health_store, task_store, task_store::TaskStore};
use crate::runtime::{captain_review, mergeability, review_phase, spawn_phase};

use super::tick_guard::{TickGuard, TICK_RUNNING};
use super::tick_spawn::default_tick_result;
pub use super::tick_spawn::{spawn_worker_for_item, ItemSpawnResult};

/// Run a single captain tick cycle (with concurrency guard).
/// Takes an Arc<RwLock> and manages locking internally — brief lock at start
/// to load items, no lock during heavy processing, brief lock at end to write back.
pub(crate) async fn run_captain_tick(
    config: &Config,
    workflow: &CaptainWorkflow,
    dry_run: bool,
    bus: Option<&EventBus>,
    emit_notifications: bool,
    store_lock: &Arc<RwLock<TaskStore>>,
) -> Result<TickResult> {
    // Generate a per-tick trace ID for cross-process log correlation.
    let tick_id = mando_uuid::Uuid::v4().short();
    let span = tracing::info_span!("captain_tick", module = "captain", tick_id = %tick_id);

    // Acquire tick guard — return early if another tick is already running.
    if TICK_RUNNING.swap(true, std::sync::atomic::Ordering::AcqRel) {
        tracing::warn!(module = "captain", tick_id = %tick_id, "tick already in progress, skipping");
        return Ok(TickResult {
            mode: "skipped".into(),
            tick_id: Some(tick_id),
            error: Some("tick already in progress".into()),
            ..default_tick_result()
        });
    }
    let _guard = TickGuard;

    let mut result = run_captain_tick_inner(
        config,
        workflow,
        dry_run,
        bus,
        emit_notifications,
        store_lock,
    )
    .instrument(span)
    .await?;
    result.tick_id = Some(tick_id);
    Ok(result)
}

/// Inner tick logic — separated so tests can call with their own lock.
async fn run_captain_tick_inner(
    config: &Config,
    workflow: &CaptainWorkflow,
    dry_run: bool,
    bus: Option<&EventBus>,
    emit_notifications: bool,
    store_lock: &Arc<RwLock<TaskStore>>,
) -> Result<TickResult> {
    let mode = if dry_run { "dry-run" } else { "live" };
    tracing::info!(module = "captain", mode = %mode, "tick start");

    let captain = &config.captain;
    let health_path = mando_config::worker_health_path();

    let mut alerts: Vec<String> = Vec::new();
    let mut dry_actions: Vec<mando_types::captain::Action> = Vec::new();

    // Create notifier — emits BusEvent::Notification for any subscriber (TG, Electron).
    let default_slug = if captain.projects.len() == 1 {
        captain
            .projects
            .values()
            .next()
            .and_then(|pc| pc.github_repo.clone())
    } else {
        None
    };
    let notifier_bus = match bus {
        Some(b) => Arc::new(b.clone()),
        None => Arc::new(mando_shared::EventBus::new()),
    };
    let notifier = super::notify::Notifier::new(notifier_bus)
        .with_repo_slug(default_slug)
        .with_notifications_enabled(emit_notifications);

    // ── §1 LOAD ───────────────────────────────────────────────────────
    // Brief read lock to load items, then release so reads aren't blocked.

    let (mut items, indices_snapshot, pool) = {
        let store = store_lock.read().await;
        (
            store.load_all().await?,
            store.routing().await?,
            store.pool().clone(),
        )
    };
    // Snapshot item IDs + serialized state so we only write back items the tick changed.
    let pre_tick_snapshot: std::collections::HashMap<i64, serde_json::Value> = items
        .iter()
        .map(|it| (it.id, task_store::task_snapshot(it)))
        .collect();
    let mut health_state = health_store::load_health_state(&health_path);

    // Clean stale per-item operation locks.
    crate::io::item_lock::clean_stale_locks();

    // Kill orphan workers — processes tracked in health state with no matching in-progress item.
    if !dry_run {
        super::tick_action_loop::kill_orphan_workers(&indices_snapshot, &mut health_state).await;
    }

    // Linear sync: import "Todo" issues into tasks.
    if !dry_run && config.features.linear {
        match super::linear_integration::sync_linear_to_tasks(config).await {
            Ok(new_items) => {
                let filtered = super::linear_integration::filter_existing(new_items, &items);
                if !filtered.is_empty() {
                    tracing::info!(
                        module = "captain",
                        count = filtered.len(),
                        "imported items from Linear"
                    );
                    items.extend(filtered);
                }
            }
            Err(e) => tracing::debug!(module = "captain", error = %e, "linear sync skipped"),
        }
    }

    let max_workers = workflow.agent.max_concurrent;
    let active_workers = items
        .iter()
        .filter(|it| it.status == ItemStatus::InProgress && it.worker.is_some())
        .count();

    tracing::info!(
        module = "captain",
        items = items.len(),
        active_workers = active_workers,
        max_workers = max_workers,
        "tick status"
    );

    // Decision journal uses the same pool.
    let journal_db = if !dry_run {
        Some(crate::io::journal::JournalDb::new(pool.clone()))
    } else {
        None
    };
    let tick_id = mando_uuid::Uuid::v4().to_string();

    // ── §2 GATHER — context for ALL non-terminal items ────────────────

    let worker_contexts =
        review_phase::gather_worker_contexts(&mut items, config, &health_state).await;

    // Persist any PRs discovered during context gathering so they survive a crash.
    super::tick_persist::flush_discovered_prs(&items, &pre_tick_snapshot, store_lock).await;

    // ── §3 CLASSIFY — single pass over all non-terminal items ─────────
    //
    // InProgress items go through deterministic classifier + LLM review.
    // Crash detection is folded in: dead process + work not done = Nudge.
    // AwaitingReview items go through mergeability checks.
    // Other non-terminal statuses (Rework, New, Queued, Clarifying,
    // NeedsClarification, CaptainReviewing) are handled in the dispatch
    // phase below.

    let classify_result = super::tick_classify::classify_and_update_health(
        &worker_contexts,
        &items,
        &mut health_state,
        workflow,
        dry_run,
    );
    let actions_to_execute = classify_result.actions_to_execute;
    dry_actions.extend(classify_result.dry_actions);

    // Crash detection — folded into classify. Dead process with incomplete work
    // is handled by the deterministic classifier as a Nudge ("continue") action.
    // The detect_crashed_workers phase is no longer needed as a separate step.

    // Mergeability — folded into classify. AwaitingReview items are checked
    // for merge conflicts, CI status, and review threads in the same pass.
    if !dry_run {
        if let Err(e) = mergeability::check_done_mergeability(
            &mut items,
            config,
            workflow,
            &notifier,
            &mut alerts,
            &health_state,
            &pool,
        )
        .await
        {
            tracing::warn!(module = "captain", error = %e, "mergeability check failed");
        }

        // Persist any PRs discovered during mergeability check.
        super::tick_persist::flush_discovered_prs(&items, &pre_tick_snapshot, store_lock).await;
    }

    // CaptainReviewing — poll for review verdicts from async CC sessions.
    if !dry_run {
        let review_timeout_s = workflow.agent.captain_review_timeout_s;
        for item in items
            .iter_mut()
            .filter(|it| it.status == ItemStatus::CaptainReviewing)
        {
            // If item has no review session, spawn one (handles retry path
            // and items that entered CaptainReviewing without a session).
            let has_session = item
                .session_ids
                .review
                .as_deref()
                .is_some_and(|s| !s.is_empty());
            if !has_session {
                let trigger = item
                    .captain_review_trigger
                    .unwrap_or(mando_types::task::ReviewTrigger::Retry);
                item.last_activity_at = Some(mando_types::now_rfc3339());
                if let Err(e) = captain_review::spawn_review(
                    item,
                    trigger.as_str(),
                    config,
                    workflow,
                    &notifier,
                    &pool,
                )
                .await
                {
                    tracing::warn!(module = "captain", item_id = item.id, error = %e, "spawn_review failed");
                }
                continue;
            }

            if let Some(verdict) = captain_review::check_review(item) {
                if let Err(e) =
                    captain_review::apply_verdict(item, &verdict, &notifier, &pool).await
                {
                    tracing::warn!(module = "captain", item_id = item.id, error = %e, "apply_verdict failed");
                }
            } else {
                // No verdict yet — check timeout. If the item has been in
                // CaptainReviewing longer than captain_review_timeout_s,
                // treat the session as dead.
                let is_timed_out = item
                    .last_activity_at
                    .as_deref()
                    .and_then(|ts| {
                        time::OffsetDateTime::parse(
                            ts,
                            &time::format_description::well_known::Rfc3339,
                        )
                        .ok()
                    })
                    .map(|entered| {
                        let elapsed = time::OffsetDateTime::now_utc() - entered;
                        elapsed.whole_seconds() as u64 > review_timeout_s
                    })
                    .unwrap_or(true); // No timestamp = treat as timed out.

                if is_timed_out {
                    let mut fail_count = item.retry_count as u32;
                    captain_review::handle_review_error(
                        item,
                        "review session timed out without producing a verdict",
                        &mut fail_count,
                        workflow,
                        &notifier,
                        &pool,
                    )
                    .await;
                    item.retry_count = fail_count as i64;
                }
            }
        }
    }

    // CaptainMerging — poll for merge session results from async CC sessions.
    if !dry_run {
        super::captain_merge::poll_merging_items(&mut items, config, workflow, &notifier, &pool)
            .await;
    }

    // Resolve outcomes + log decisions to journal.
    if let Some(ref jdb) = journal_db {
        super::tick_journal::resolve_outcomes(jdb, &actions_to_execute, &worker_contexts).await;
        super::tick_journal::log_decisions(
            jdb,
            &tick_id,
            &actions_to_execute,
            &worker_contexts,
            &items,
            &health_state,
        )
        .await;
    }

    // ── §4 EXECUTE — all actions ──────────────────────────────────────

    if !dry_run {
        for action in &actions_to_execute {
            if let Err(e) = spawn_phase::execute_action(
                action,
                &mut items,
                config,
                workflow,
                &notifier,
                &mut alerts,
                &pool,
            )
            .await
            {
                alerts.push(format!(
                    "Action {:?} failed for {}: {}",
                    action.action, action.worker, e
                ));
            }
        }
    }

    // Dispatch: Rework → Queued, then spawn workers for Queued/New items.
    let mut active_workers = items
        .iter()
        .filter(|it| it.status == ItemStatus::InProgress && it.worker.is_some())
        .count();

    let mut dry_dispatch_actions: Vec<String> = Vec::new();

    if !dry_run {
        // Rework → Queued: clear worker fields.
        for item in items.iter_mut() {
            if item.status == ItemStatus::Rework {
                let item_id = item.id.to_string();
                let _lock = if item.id > 0 {
                    match crate::io::item_lock::acquire_item_lock(&item_id, "tick-rework-dispatch")
                    {
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
                item.worker_started_at = None;
                item.session_ids.worker = None;
                tracing::info!(
                    module = "captain",
                    title = %&item.title[..item.title.len().min(60)],
                    "dispatch: rework to queued"
                );
            }
        }

        // Dispatch Queued items to workers + New items to clarifier.
        let resource_limits = &workflow.agent.resource_limits;
        active_workers = super::dispatch_phase::dispatch_new_work(
            &mut items,
            config,
            active_workers,
            max_workers,
            workflow,
            &notifier,
            false,
            &mut dry_dispatch_actions,
            &mut alerts,
            resource_limits,
            &pool,
        )
        .await?;

        // Brief write lock: only write back items the tick actually modified.
        {
            let changed_items: Vec<mando_types::Task> = items
                .iter()
                .filter(|item| {
                    let current_snapshot = task_store::task_snapshot(item);
                    match pre_tick_snapshot.get(&item.id) {
                        Some(old_snapshot) => *old_snapshot != current_snapshot,
                        None => true,
                    }
                })
                .cloned()
                .collect();
            if !changed_items.is_empty() {
                let store = store_lock.write().await;
                store
                    .merge_changed_items(&pre_tick_snapshot, &changed_items)
                    .await?;
            }
        }
    }

    // ── §5 POST — persist, SSE, prune ─────────────────────────────────

    super::tick_post::run_post_phase(
        dry_run,
        &health_path,
        &health_state,
        &journal_db,
        &notifier,
        bus,
    )
    .await?;

    // Archive terminal tasks that have been finalized longer than the grace period.
    if !dry_run {
        let store = store_lock.read().await;
        match store
            .archive_terminal(workflow.agent.archive_grace_secs)
            .await
        {
            Ok(n) if n > 0 => {
                tracing::info!(module = "captain", archived = n, "archived terminal tasks");
            }
            Err(e) => {
                tracing::warn!(module = "captain", error = %e, "archive terminal tasks failed");
            }
            _ => {}
        }
    }

    // Reconcile stale "running" sessions against stream ground truth.
    if !dry_run {
        let store = store_lock.read().await;
        super::session_reconcile::reconcile_running_sessions(store.pool()).await;
    }

    let status_counts = {
        store_lock
            .read()
            .await
            .status_counts()
            .await
            .unwrap_or_default()
    };
    super::tick_post::log_tick_summary(&status_counts, active_workers, alerts.len());

    Ok(TickResult {
        mode: mode.into(),
        tick_id: None, // set by caller (run_captain_tick)
        max_workers,
        active_workers,
        tasks: status_counts,
        alerts,
        dry_actions,
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_workflow() -> CaptainWorkflow {
        CaptainWorkflow::compiled_default()
    }

    async fn test_store_lock(_dir: &std::path::Path) -> Arc<RwLock<TaskStore>> {
        let db = mando_db::Db::open_in_memory().await.unwrap();
        let store = TaskStore::new(db.pool().clone());
        Arc::new(RwLock::new(store))
    }

    #[tokio::test]
    async fn tick_no_tasks() {
        let dir = std::env::temp_dir().join("mando-tick-test-none");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store_lock = test_store_lock(&dir).await;
        let config = Config::default();
        let wf = test_workflow();
        let result = run_captain_tick_inner(&config, &wf, true, None, true, &store_lock)
            .await
            .unwrap();
        assert_eq!(result.mode, "dry-run");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn tick_dry_run_does_not_mutate() {
        let dir = std::env::temp_dir().join("mando-tick-test-dry");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store_lock = test_store_lock(&dir).await;
        {
            let store = store_lock.write().await;
            let mut t = mando_types::Task::new("Test task");
            t.status = ItemStatus::New;
            store.add(t).await.unwrap();
        }
        let config = Config::default();
        let wf = test_workflow();
        let result = run_captain_tick_inner(&config, &wf, true, None, true, &store_lock)
            .await
            .unwrap();
        assert_eq!(result.mode, "dry-run");
        assert!(result.error.is_none());
        assert_eq!(result.tasks.get("new"), Some(&1));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn tick_live_retries_clarifier_on_failure() {
        let dir = std::env::temp_dir().join("mando-tick-test-live");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store_lock = test_store_lock(&dir).await;
        {
            let store = store_lock.write().await;
            let mut t = mando_types::Task::new("Lifecycle test item");
            t.status = ItemStatus::New;
            t.project = Some("acme/widgets".into());
            store.add(t).await.unwrap();
        }
        let config = Config::default();
        let wf = test_workflow();

        // Hide the claude binary so the clarifier fails.
        let orig_path = std::env::var("PATH").unwrap_or_default();
        let orig_home = std::env::var("HOME").unwrap_or_default();
        std::env::set_var("PATH", "/nonexistent");
        std::env::set_var("HOME", "/nonexistent");

        let result = run_captain_tick_inner(&config, &wf, false, None, true, &store_lock)
            .await
            .unwrap();

        std::env::set_var("PATH", &orig_path);
        std::env::set_var("HOME", &orig_home);

        assert_eq!(result.mode, "live");
        assert!(result.error.is_none());
        // Task stays New (retryable), not auto-promoted to Ready.
        assert_eq!(result.tasks.get("new"), Some(&1));
        assert_eq!(result.tasks.get("ready"), None);

        // Verify spawn_fail_count incremented.
        let store = store_lock.read().await;
        let task = store.find_by_id(1).await.unwrap().unwrap();
        assert_eq!(task.retry_count, 1);
        std::fs::remove_dir_all(&dir).ok();
    }
}
