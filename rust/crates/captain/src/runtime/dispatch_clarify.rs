//! Parallel clarification dispatch — spawns clarifier sessions as background tasks.

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;

use crate::{ItemStatus, Task};
use futures::FutureExt;
use global_bus::EventBus;
use settings::config::settings::Config;
use settings::config::workflow::CaptainWorkflow;
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

    // Phase 1: Pre-process — set Clarifying status, persist to DB, log sessions.
    let mut jobs: Vec<ClarifyJob> = Vec::new();
    for idx in new_items {
        if jobs.len() >= max_parallel {
            break;
        }
        if dry_run {
            dry_actions.push(format!(
                "would clarify '{}'",
                truncate_utf8(&items[idx].title, 60)
            ));
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
