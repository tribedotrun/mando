//! Captain tick entry point — `run_captain_tick()`.
//!
//! 5-phase single-pass tick:
//! §1 LOAD — all non-terminal items + health state + kill orphans
//! §2 GATHER — context for ALL non-terminal items
//! §3 CLASSIFY — one pass, all items, produces action list
//! §4 EXECUTE — all actions
//! §5 POST — persist, SSE, prune

use std::sync::Arc;

use anyhow::{Context, Result};
use tracing::Instrument;

use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_shared::EventBus;
use mando_types::captain::{TickMode, TickResult};
use mando_types::task::ItemStatus;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use super::tick_guard::{TickRunningGuard, TICK_RUNNING};
use super::tick_spawn::default_tick_result;
pub use super::tick_spawn::{spawn_worker_for_item, ItemSpawnResult};
use crate::io::{health_store, task_store, task_store::TaskStore};
use crate::runtime::{mergeability, review_phase, spawn_phase};

/// SIGTERM cancellation checkpoint -- returns a Skipped result when cancelled.
fn cancelled_result(cancel: &CancellationToken, phase: &str) -> Option<TickResult> {
    if !cancel.is_cancelled() {
        return None;
    }
    tracing::info!(module = "captain", "tick cancelled after {phase}");
    Some(TickResult {
        mode: TickMode::Skipped,
        ..default_tick_result()
    })
}

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
    cancel: &CancellationToken,
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
    // RAII guard: clears TICK_RUNNING on drop (normal exit or panic).
    let _guard = TickRunningGuard;
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
        cancel,
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
    cancel: &CancellationToken,
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
    // Sync branch from worktrees before snapshotting (mando-pr renames branches).
    if !dry_run {
        super::tick_branch_sync::sync_branches(&mut items).await;
    }

    // Snapshot item state so we only write back items the tick changed.
    let mut pre_tick_snapshot: std::collections::HashMap<i64, serde_json::Value> =
        std::collections::HashMap::with_capacity(items.len());
    for it in items.iter() {
        let snap = task_store::task_snapshot(it).with_context(|| {
            format!("tick pre-snapshot serialization failed for task {}", it.id)
        })?;
        pre_tick_snapshot.insert(it.id, snap);
    }
    let mut health_state = health_store::load_health_state(&health_path)
        .with_context(|| format!("load health state from {}", health_path.display()))?;

    // Clean stale per-item operation locks.
    crate::io::item_lock::clean_stale_locks();

    // Kill orphan workers — processes tracked in health state with no matching in-progress item.
    // Returns removed worker names so the POST phase can clean them from disk.
    let removed_workers = if !dry_run {
        super::tick_action_loop::kill_orphan_workers(&indices_snapshot, &mut health_state, &pool)
            .await
    } else {
        Vec::new()
    };

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
    // Two independent systems:
    // - Credentials configured: blocked when all credentials are rate-limited
    //   (pick_for_worker returns None).
    // - No credentials: blocked by ambient (host login) cooldown.
    let has_credentials = mando_db::queries::credentials::has_any(&pool)
        .await
        .unwrap_or(false);
    let rate_limited = if has_credentials {
        // All credentials rate-limited = no available credential to pick.
        mando_db::queries::credentials::pick_for_worker(&pool)
            .await
            .unwrap_or(None)
            .is_none()
    } else {
        super::ambient_rate_limit::is_active()
    };
    if rate_limited {
        let remaining = if has_credentials {
            0 // per-credential cooldowns have their own DB timestamps
        } else {
            super::ambient_rate_limit::remaining_secs()
        };
        tracing::warn!(
            module = "captain",
            remaining_s = remaining,
            "rate limit cooldown active — CC session spawning suppressed"
        );
    }

    // ── §2 GATHER — context for ALL non-terminal items ────────────────

    let worker_contexts =
        review_phase::gather_worker_contexts(&mut items, config, &health_state, &pool).await?;

    // Persist any PRs discovered during context gathering so they survive a crash.
    super::tick_persist::flush_discovered_prs(&items, &pre_tick_snapshot, store_lock, &mut alerts)
        .await;

    // ── SIGTERM checkpoint: between §2 GATHER and §3 CLASSIFY ─────────
    if let Some(r) = cancelled_result(cancel, "GATHER phase") {
        return Ok(r);
    }

    // ── §3 CLASSIFY — single pass over all non-terminal items ─────────

    let classify_result = super::tick_classify::classify_and_update_health(
        &worker_contexts,
        &items,
        &mut health_state,
        workflow,
        dry_run,
    )?;
    let actions_to_execute = classify_result.actions_to_execute;
    dry_actions.extend(classify_result.dry_actions);

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
        super::tick_persist::flush_discovered_prs(
            &items,
            &pre_tick_snapshot,
            store_lock,
            &mut alerts,
        )
        .await;
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

    // Clarifying — poll for results from async clarifier sessions.
    if !dry_run {
        super::tick_clarify_poll::poll_clarifying_items(
            &mut items,
            config,
            workflow,
            &notifier,
            &pool,
            rate_limited,
            &workflow.agent.resource_limits,
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

    // ── SIGTERM checkpoint: between §3 CLASSIFY and §4 EXECUTE ─────────
    if let Some(r) = cancelled_result(cancel, "CLASSIFY phase") {
        return Ok(r);
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
        super::tick_rework::transition_rework_to_queued(&mut items, &mut alerts);

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
                bus,
            )
            .await;
        }

        // ── SIGTERM checkpoint: between §4 EXECUTE and write-back ───────
        if let Some(r) = cancelled_result(cancel, "EXECUTE phase") {
            return Ok(r);
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
                        Err(e) => {
                            tracing::debug!(task_id = item.id, error = %e, "snapshot failed, treating as changed");
                            true
                        }
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
                        // Escalate loudly — a stuck write lock means a reader
                        // is wedged and the tick cannot persist its progress.
                        // Fire a CRITICAL notification so operators see it on
                        // Telegram / Electron instead of only in the log, and
                        // return an Err so the auto-tick loop increments its
                        // consecutive-failure counter.
                        tracing::error!(
                            module = "captain",
                            "tick write-lock timed out after 30s — aborting merge, task_store reader wedged"
                        );
                        notifier
                            .critical(
                                "Captain tick aborted: task store write lock timed out after 30s. \
                                 A reader task is wedged — investigate active /api handlers and SSE subscribers.",
                            )
                            .await;
                        return Err(anyhow::anyhow!(
                            "tick write-lock timed out after 30s (task_store reader wedged)"
                        ));
                    }
                };
                store
                    .merge_changed_items(&pre_tick_snapshot, &changed_items)
                    .await?;
            }
        }
    }

    // ── §5 POST — persist, SSE, prune ─────────────────────────────────

    let affected_task_ids: Vec<i64> = items.iter().map(|t| t.id).collect();
    super::tick_post::run_post_phase(
        dry_run,
        &health_path,
        &health_state,
        &removed_workers,
        &notifier,
        bus,
        &affected_task_ids,
    )
    .await?;

    super::tick_post::run_post_cleanup(dry_run, store_lock, workflow, &mut alerts).await;

    let status_counts = match store_lock.read().await.status_counts().await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(
                module = "captain",
                error = %e,
                "tick status_counts failed; surfacing as alert"
            );
            alerts.push(format!("tick status_counts query failed: {e}"));
            std::collections::HashMap::new()
        }
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
