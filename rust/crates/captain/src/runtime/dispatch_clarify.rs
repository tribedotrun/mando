//! Parallel clarification dispatch — spawns clarifier sessions as background tasks.

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;

use crate::{ItemStatus, Task};
use futures::FutureExt;
use global_bus::EventBus;
use settings::CaptainWorkflow;
use settings::Config;
use tokio_util::task::TaskTracker;

use crate::runtime::clarifier;
use crate::runtime::dashboard::truncate_utf8;
use crate::runtime::notify::Notifier;
use crate::service::{dispatch_logic, lifecycle};

struct ClarifyJob {
    idx: usize,
    session_id: String,
}

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all)]
pub(crate) async fn clarify_new_items(
    items: &mut [Task],
    config: &Config,
    active_workers: usize,
    max_workers: usize,
    workflow: &CaptainWorkflow,
    _notifier: &Notifier,
    dry_run: bool,
    dry_actions: &mut Vec<String>,
    _alerts: &mut Vec<String>,
    _resource_limits: &HashMap<String, usize>,
    _max_clarifier_retries: i64,
    pool: &sqlx::SqlitePool,
    bus: Option<&EventBus>,
    task_tracker: &TaskTracker,
) {
    let new_items = dispatch_logic::new_items(items);
    if new_items.is_empty() {
        return;
    }

    // Skip clarification entirely when no worker slots are available.
    if active_workers >= max_workers {
        return;
    }

    // Cap parallel clarifications at available worker slots to avoid
    // overwhelming the LLM provider with a burst of concurrent sessions.
    let max_parallel = max_workers.saturating_sub(active_workers);

    // Per-state cap for `clarifying`: count items already running a
    // clarifier session, then refuse to start more once the cap is hit.
    let per_state_limits = &workflow.agent.per_state_limits;
    let clarifying_cap = per_state_limits.get("clarifying").copied();
    let mut clarifying_active = dispatch_logic::count_active_states(items)
        .get("clarifying")
        .copied()
        .unwrap_or(0);

    // Phase 1: Pre-process — set Clarifying status, persist to DB, log sessions.
    let mut jobs: Vec<ClarifyJob> = Vec::new();
    for idx in new_items {
        if jobs.len() >= max_parallel {
            break;
        }
        if let Some(cap) = clarifying_cap {
            if clarifying_active >= cap {
                tracing::debug!(
                    module = "captain",
                    state = "clarifying",
                    current = clarifying_active,
                    cap = cap,
                    title = %items[idx].title,
                    "per-state cap reached — deferring dispatch"
                );
                break;
            }
        }
        if dry_run {
            dry_actions.push(format!(
                "would clarify '{}'",
                truncate_utf8(&items[idx].title, 60)
            ));
            // Reserve the cap slot so subsequent iterations in this dry
            // run see the projected count and stop at the cap, matching
            // what the live path does once the job is pushed.
            clarifying_active += 1;
            continue;
        }

        let session_id = global_infra::uuid::Uuid::v4().to_string();
        let item = &mut items[idx];
        if let Err(e) = lifecycle::apply_transition(item, ItemStatus::Clarifying) {
            tracing::error!(
                module = "captain",
                item_id = item.id,
                error = %e,
                "illegal clarify-start transition"
            );
            continue;
        }
        item.last_activity_at = Some(global_types::now_rfc3339());
        item.session_ids.clarifier = Some(session_id.clone());

        if let Err(e) = crate::io::queries::tasks::persist_clarify_start(pool, item).await {
            tracing::error!(
                module = "captain",
                id = item.id,
                error = %e,
                "failed to persist clarify start — skipping clarifier this tick"
            );
            lifecycle::restore_status(item, ItemStatus::New);
            item.session_ids.clarifier = None;
            continue;
        }

        let clarifier_cwd = match crate::runtime::clarifier::resolve_clarifier_cwd(item, config) {
            Ok(cwd) => cwd,
            Err(e) => {
                tracing::error!(
                    module = "captain",
                    id = item.id,
                    error = %e,
                    "cannot log clarifier session start, skipping this dispatch"
                );
                item.session_ids.clarifier = None;
                continue;
            }
        };
        if let Err(e) = crate::io::headless_cc::log_cc_session(
            pool,
            &crate::io::headless_cc::SessionLogEntry {
                session_id: &session_id,
                cwd: &clarifier_cwd,
                model: &workflow.models.clarifier,
                caller: "clarifier",
                cost_usd: None,
                duration_ms: None,
                resumed: false,
                task_id: Some(item.id),
                status: global_types::SessionStatus::Running,
                worker_name: "",
                credential_id: None,
                error: None,
                api_error_status: None,
            },
        )
        .await
        {
            tracing::warn!(module = "captain", id = item.id, error = %e, "failed to log clarifier session start");
        }

        global_infra::best_effort!(
            super::timeline_emit::emit_for_task(
                item,
                "Clarification starting",
                crate::TimelineEventPayload::ClarifyStarted {
                    session_id: session_id.clone(),
                },
                pool,
            )
            .await,
            "dispatch_clarify: super::timeline_emit::emit_for_task( item, 'Clarification st"
        );
        clarifying_active += 1;
        jobs.push(ClarifyJob { idx, session_id });
    }

    if jobs.is_empty() {
        return;
    }

    let clarified_ids: Vec<i64> = jobs.iter().map(|j| items[j.idx].id).collect();
    emit_live_refresh(bus, &clarified_ids);

    // Phase 2: Spawn all clarifications as detached async tasks.
    // Each task runs the CC session and writes results to the stream file.
    // The tick continues immediately — results are polled by
    // tick_clarify_poll on subsequent ticks.
    for job in jobs {
        let task = items[job.idx].clone();
        let workflow = workflow.clone();
        let config = config.clone();
        let pool = pool.clone();
        let session_id = job.session_id.clone();

        let session_id_for_panic = session_id.clone();
        let cwd = match clarifier::resolve_clarifier_cwd(&task, &config) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(
                    module = "captain",
                    id = task.id,
                    error = %e,
                    "cannot resolve cwd for async clarifier — writing error"
                );
                let stream_path = global_infra::paths::stream_path_for_session(&session_id);
                global_claude::write_error_result(
                    &stream_path,
                    &format!("cannot resolve clarifier cwd: {e}"),
                );
                continue;
            }
        };
        let cwd_for_failure = cwd.clone();
        let task_id_num = task.id;

        task_tracker.spawn(async move {
            let result = AssertUnwindSafe(async {
                match clarifier::run_clarification(
                    &task,
                    &workflow,
                    &config,
                    &pool,
                    Some(&session_id),
                )
                .await
                {
                    Ok(_result) => {
                        // run_clarification already logged the session as Stopped
                        // and the CC process wrote results to the stream file.
                        tracing::info!(
                            module = "captain",
                            %session_id,
                            "async clarifier completed"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            module = "captain",
                            %session_id,
                            error = %e,
                            "async clarifier failed"
                        );
                        // If the failover layer exhausted all healthy
                        // credentials, park the task with `paused_until`
                        // set to the soonest cooldown. Captain tick will
                        // exclude it from dispatch until the clock passes.
                        if let Some(global_claude::CcError::AllCredentialsExhausted {
                            earliest_reset,
                        }) = e.downcast_ref::<global_claude::CcError>()
                        {
                            if let Err(e2) = crate::io::queries::tasks::set_paused_until(
                                &pool,
                                task_id_num,
                                *earliest_reset,
                            )
                            .await
                            {
                                tracing::warn!(
                                    module = "captain",
                                    task_id = task_id_num,
                                    error = %e2,
                                    "failed to set paused_until on AllCredentialsExhausted"
                                );
                            } else {
                                tracing::warn!(
                                    module = "captain",
                                    task_id = task_id_num,
                                    earliest_reset,
                                    "task paused — all credentials rate-limited"
                                );
                            }
                        }
                        let stream_path = global_infra::paths::stream_path_for_session(&session_id);
                        global_claude::write_error_result(
                            &stream_path,
                            &format!("clarifier failed: {e}"),
                        );
                        let error_text = format!("{e}");
                        let api_error_status = e
                            .downcast_ref::<global_claude::CcError>()
                            .and_then(|cc| cc.api_error_status());
                        if let Err(e2) = crate::io::headless_cc::log_cc_failure(
                            &pool,
                            &session_id,
                            &cwd_for_failure,
                            "clarifier",
                            Some(task_id_num),
                            Some(&error_text),
                            api_error_status,
                        )
                        .await
                        {
                            tracing::warn!(
                                module = "captain",
                                %session_id,
                                error = %e2,
                                "log_cc_failure failed"
                            );
                        }
                    }
                }
            })
            .catch_unwind()
            .await;

            if let Err(panic) = result {
                tracing::error!(
                    module = "captain",
                    session_id = %session_id_for_panic,
                    "async clarifier panicked: {:?}",
                    panic
                );
                let stream_path =
                    global_infra::paths::stream_path_for_session(&session_id_for_panic);
                global_claude::write_error_result(
                    &stream_path,
                    &format!("clarifier panicked: {:?}", panic),
                );
            }
        });
    }
}

fn emit_live_refresh(bus: Option<&EventBus>, affected_task_ids: &[i64]) {
    if let Some(bus) = bus {
        bus.send(global_bus::BusPayload::Tasks(None));
        bus.send(global_bus::BusPayload::Sessions(Some(
            api_types::SessionsEventData {
                affected_task_ids: Some(affected_task_ids.to_vec()),
            },
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> sqlx::SqlitePool {
        let db = global_db::Db::open_in_memory().await.unwrap();
        db.pool().clone()
    }

    /// Per-state cap on `clarifying` defers a New item from entering
    /// the clarifier path even when worker slots are available. Worker
    /// dispatch is unaffected — covered by dispatch_phase_tests.
    #[tokio::test]
    async fn clarify_dry_run_defers_when_clarifying_cap_full() {
        let pool = test_pool().await;
        // Two New items + an item already Clarifying (occupying the cap).
        let mut live = Task::new("Live clarifier");
        live.id = 1;
        live.status = ItemStatus::Clarifying;
        live.session_ids.clarifier = Some("sess-c".into());

        let mut new_a = Task::new("New A");
        new_a.id = 2;
        new_a.status = ItemStatus::New;
        let mut new_b = Task::new("New B");
        new_b.id = 3;
        new_b.status = ItemStatus::New;

        let config = Config::default();
        let mut workflow = CaptainWorkflow::compiled_default();
        workflow
            .agent
            .per_state_limits
            .insert("clarifying".into(), 1);
        let notifier = Notifier::new(std::sync::Arc::new(global_bus::EventBus::new()));
        let mut items = vec![live, new_a, new_b];
        let mut dry = Vec::new();
        let mut alerts = Vec::new();

        clarify_new_items(
            &mut items,
            &config,
            0,
            10,
            &workflow,
            &notifier,
            true,
            &mut dry,
            &mut alerts,
            &HashMap::new(),
            3,
            &pool,
            None,
            &tokio_util::task::TaskTracker::new(),
        )
        .await;

        assert!(
            dry.is_empty(),
            "no clarifier dispatches should run while clarifying cap is full, got {:?}",
            dry
        );
    }

    /// Multi-item dry run honors the clarifying cap: with two New items
    /// and a clarifying cap of 1 (no live clarifier), exactly one item
    /// goes into the dry-action list. Without bumping
    /// `clarifying_active` on the dry-run continue, both items would
    /// erroneously be reported as "would clarify".
    #[tokio::test]
    async fn clarify_dry_run_caps_count_across_iterations() {
        let pool = test_pool().await;

        let mut a = Task::new("New A");
        a.id = 1;
        a.status = ItemStatus::New;
        let mut b = Task::new("New B");
        b.id = 2;
        b.status = ItemStatus::New;

        let config = Config::default();
        let mut workflow = CaptainWorkflow::compiled_default();
        workflow
            .agent
            .per_state_limits
            .insert("clarifying".into(), 1);
        let notifier = Notifier::new(std::sync::Arc::new(global_bus::EventBus::new()));
        let mut items = vec![a, b];
        let mut dry = Vec::new();
        let mut alerts = Vec::new();

        clarify_new_items(
            &mut items,
            &config,
            0,
            10,
            &workflow,
            &notifier,
            true,
            &mut dry,
            &mut alerts,
            &HashMap::new(),
            3,
            &pool,
            None,
            &tokio_util::task::TaskTracker::new(),
        )
        .await;

        assert_eq!(dry.len(), 1, "dry run must respect cap=1, got {dry:?}");
    }

    #[tokio::test]
    async fn clarify_dry_run_proceeds_when_under_cap() {
        let pool = test_pool().await;
        let mut new_a = Task::new("New A");
        new_a.id = 1;
        new_a.status = ItemStatus::New;
        let mut new_b = Task::new("New B");
        new_b.id = 2;
        new_b.status = ItemStatus::New;

        let config = Config::default();
        let mut workflow = CaptainWorkflow::compiled_default();
        workflow
            .agent
            .per_state_limits
            .insert("clarifying".into(), 5);
        let notifier = Notifier::new(std::sync::Arc::new(global_bus::EventBus::new()));
        let mut items = vec![new_a, new_b];
        let mut dry = Vec::new();
        let mut alerts = Vec::new();

        clarify_new_items(
            &mut items,
            &config,
            0,
            10,
            &workflow,
            &notifier,
            true,
            &mut dry,
            &mut alerts,
            &HashMap::new(),
            3,
            &pool,
            None,
            &tokio_util::task::TaskTracker::new(),
        )
        .await;

        assert_eq!(dry.len(), 2, "both items should clarify under cap=5");
    }
}
