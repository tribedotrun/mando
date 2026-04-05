//! Captain tick entry point — `run_captain_tick()`.
//!
//! 5-phase single-pass tick:
//! §1 LOAD — all non-terminal items + health state + kill orphans
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
use mando_types::captain::{TickMode, TickResult};
use mando_types::task::ItemStatus;
use tokio::sync::RwLock;

use crate::io::{health_store, task_store, task_store::TaskStore};
use crate::runtime::{mergeability, review_phase, spawn_phase};

use super::tick_guard::{acquire_tick_guard, TICK_RUNNING};
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
            mode: TickMode::Skipped,
            tick_id: Some(tick_id),
            error: Some("tick already in progress".into()),
            ..default_tick_result()
        });
    }
    let _guard = match acquire_tick_guard() {
        Ok(guard) => guard,
        Err(e) => {
            TICK_RUNNING.store(false, std::sync::atomic::Ordering::Release);
            tracing::warn!(module = "captain", tick_id = %tick_id, error = %e, "captain tick lock held elsewhere, skipping");
            return Ok(TickResult {
                mode: TickMode::Skipped,
                tick_id: Some(tick_id),
                error: Some(e.to_string()),
                ..default_tick_result()
            });
        }
    };
    let _file_lock = match crate::io::captain_lock::try_acquire() {
        Ok(lock) => lock,
        Err(e) => {
            tracing::warn!(module = "captain", tick_id = %tick_id, error = %e, "captain file lock held, skipping");
            return Ok(TickResult {
                mode: TickMode::Skipped,
                tick_id: Some(tick_id),
                error: Some(e.to_string()),
                ..default_tick_result()
            });
        }
    };

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
    let mode = if dry_run {
        TickMode::DryRun
    } else {
        TickMode::Live
    };
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
        .filter_map(|it| task_store::task_snapshot(it).ok().map(|s| (it.id, s)))
        .collect();
    let mut health_state = health_store::load_health_state(&health_path);

    // Clean stale per-item operation locks.
    crate::io::item_lock::clean_stale_locks();

    // Kill orphan workers — processes tracked in health state with no matching in-progress item.
    if !dry_run {
        super::tick_action_loop::kill_orphan_workers(&indices_snapshot, &mut health_state, &pool)
            .await;
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

    // ── Rate-limit cooldown check ──────────────────────────────────────
    let rate_limited = super::rate_limit_cooldown::is_active();
    if rate_limited {
        let remaining = super::rate_limit_cooldown::remaining_secs();
        tracing::warn!(
            module = "captain",
            remaining_s = remaining,
            "rate limit cooldown active — CC session spawning suppressed"
        );
    }

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
        super::tick_review::poll_reviewing_items(
            &mut items,
            config,
            workflow,
            &notifier,
            &pool,
            rate_limited,
        )
        .await;
    }

    // NeedsClarification timeout — escalate items waiting too long for human answers.
    if !dry_run {
        super::tick_clarify_timeout::check_clarifier_timeouts(
            &mut items, workflow, &notifier, &pool,
        )
        .await;
    }

    // CaptainMerging — poll for merge session results from async CC sessions.
    if !dry_run {
        super::captain_merge_poll::poll_merging_items(
            &mut items,
            config,
            workflow,
            &notifier,
            &pool,
            rate_limited,
        )
        .await;
    }

    // ── §4 EXECUTE — all actions ──────────────────────────────────────

    if !dry_run {
        for action in &actions_to_execute {
            // During rate-limit cooldown, skip actions that spawn CC sessions.
            // Ship is fine (status transition only). Nudge and CaptainReview
            // would spawn sessions that immediately fail.
            if rate_limited {
                use mando_types::captain::ActionKind;
                if matches!(action.action, ActionKind::Nudge | ActionKind::CaptainReview) {
                    tracing::debug!(
                        module = "captain",
                        action = ?action.action,
                        worker = %action.worker,
                        "skipping action during rate-limit cooldown"
                    );
                    continue;
                }
            }
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
        super::tick_rework::transition_rework_to_queued(&mut items);

        // Dispatch Queued items to workers + New items to clarifier.
        // Skip entirely during rate-limit cooldown — no CC sessions will be spawned.
        if !rate_limited {
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
        }

        // Brief write lock: only write back items the tick actually modified.
        {
            let changed_items: Vec<mando_types::Task> = items
                .iter()
                .filter(|item| {
                    match task_store::task_snapshot(item) {
                        Ok(current_snapshot) => match pre_tick_snapshot.get(&item.id) {
                            Some(old_snapshot) => *old_snapshot != current_snapshot,
                            None => true,
                        },
                        Err(_) => true, // can't snapshot — treat as changed
                    }
                })
                .cloned()
                .collect();
            if !changed_items.is_empty() {
                let store = match tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    store_lock.write(),
                )
                .await
                {
                    Ok(guard) => guard,
                    Err(_) => {
                        tracing::error!(
                            module = "captain",
                            "tick write-lock timed out after 30s — aborting merge"
                        );
                        return Err(anyhow::anyhow!("tick write-lock timed out after 30s"));
                    }
                };
                store
                    .merge_changed_items(&pre_tick_snapshot, &changed_items)
                    .await?;
            }
        }
    }

    // ── §5 POST — persist, SSE, prune ─────────────────────────────────

    super::tick_post::run_post_phase(dry_run, &health_path, &health_state, &notifier, bus).await?;

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
        super::session_reconcile::reconcile_running_sessions(
            store.pool(),
            workflow.agent.stale_threshold_s,
        )
        .await;
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
        mode,
        tick_id: None, // set by caller (run_captain_tick)
        max_workers,
        active_workers,
        tasks: status_counts,
        alerts,
        dry_actions,
        error: None,
        rate_limited,
    })
}

#[cfg(test)]
#[path = "tick_tests.rs"]
mod tests;
